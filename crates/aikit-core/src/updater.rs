use std::{
    fs,
    io::{copy, Write},
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tar::Archive;
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use zip::ZipArchive;

use crate::{AikitError, Result};

mod install;
pub use install::{
    cleanup_previous_binary, install_binary, install_binary_with_backup,
    prepare_binary_installation, BinaryInstallation, PreparedBinaryInstallation,
};

pub const UPDATE_CHECK_COOLDOWN: Duration = Duration::hours(24);

pub const LATEST_RELEASE_URL: &str = "https://github.com/millylee/aikit/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UpdateCheckOutcome {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAssets {
    pub tag_name: String,
    pub archive_name: String,
    pub archive_url: String,
    pub checksum_name: String,
    pub checksum_url: String,
}

pub fn release_target_triple() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        ("windows", "aarch64") => Ok("aarch64-pc-windows-msvc"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        (os, arch) => Err(AikitError::Provider(format!(
            "unsupported platform for updates: {os}-{arch}"
        ))),
    }
}

pub fn release_archive_name() -> Result<String> {
    let triple = release_target_triple()?;
    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    Ok(format!("aikit-{triple}.{ext}"))
}

pub fn binary_file_name() -> &'static str {
    if cfg!(windows) {
        "aikit.exe"
    } else {
        "aikit"
    }
}

pub fn normalize_release_tag(tag: &str) -> String {
    tag.trim().trim_start_matches('v').to_string()
}

pub fn update_check_timestamp_now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}

pub fn update_check_cooldown_active(last_checked_at: Option<&str>) -> bool {
    let Some(raw) = last_checked_at else {
        return false;
    };
    let Ok(parsed) = OffsetDateTime::parse(raw, &Rfc3339) else {
        return false;
    };
    OffsetDateTime::now_utc() - parsed < UPDATE_CHECK_COOLDOWN
}

pub fn version_is_newer(candidate: &str, current: &str) -> bool {
    compare_versions(candidate, current).is_gt()
}

pub fn parse_sha256_file(content: &str) -> Result<String> {
    let line = content
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| AikitError::Provider("checksum file is empty".into()))?;
    let hash = line
        .split_whitespace()
        .next()
        .ok_or_else(|| AikitError::Provider("checksum file missing hash".into()))?;
    if hash.len() != 64 || !hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(AikitError::Provider(format!(
            "invalid sha256 checksum: {hash}"
        )));
    }
    Ok(hash.to_ascii_lowercase())
}

pub async fn check_for_updates(
    client: &Client,
    latest_release_url: &str,
) -> Result<UpdateCheckOutcome> {
    let tag_name = fetch_latest_release_tag(client, latest_release_url).await?;
    update_check_outcome(&tag_name)
}

fn update_check_outcome(tag_name: &str) -> Result<UpdateCheckOutcome> {
    let latest_version = normalize_release_tag(tag_name);
    if latest_version.is_empty() {
        return Err(AikitError::Provider(
            "latest release does not include a tag_name".into(),
        ));
    }

    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let update_available = version_is_newer(&latest_version, &current_version);
    let message = if update_available {
        format!("Update available: v{latest_version} (current v{current_version})")
    } else {
        format!("Already up to date: v{current_version}")
    };

    Ok(UpdateCheckOutcome {
        current_version,
        latest_version,
        update_available,
        message,
    })
}

pub async fn fetch_release_assets(
    client: &Client,
    latest_release_url: &str,
) -> Result<ReleaseAssets> {
    let tag_name = fetch_latest_release_tag(client, latest_release_url).await?;
    release_assets_for_tag(latest_release_url, tag_name)
}

fn release_assets_for_tag(latest_release_url: &str, tag_name: String) -> Result<ReleaseAssets> {
    let archive_name = release_archive_name()?;
    let checksum_name = format!("{archive_name}.sha256");

    let repo_base = latest_release_url
        .trim_end_matches("/releases/latest")
        .trim_end_matches('/');
    let archive_url = format!("{repo_base}/releases/download/{tag_name}/{archive_name}");
    let checksum_url = format!("{repo_base}/releases/download/{tag_name}/{checksum_name}");

    Ok(ReleaseAssets {
        tag_name,
        archive_name,
        archive_url,
        checksum_name,
        checksum_url,
    })
}

pub async fn download_and_stage(client: &Client, latest_release_url: &str) -> Result<PathBuf> {
    let assets = fetch_release_assets(client, latest_release_url).await?;
    let staged = download_and_stage_assets(client, &assets).await?;
    Ok(staged.keep().join(binary_file_name()))
}

async fn download_and_stage_assets(
    client: &Client,
    assets: &ReleaseAssets,
) -> Result<tempfile::TempDir> {
    let archive_bytes = download_bytes(client, &assets.archive_url).await?;
    let checksum_bytes = download_bytes(client, &assets.checksum_url).await?;
    let checksum_text = String::from_utf8(checksum_bytes)
        .map_err(|err| AikitError::Provider(format!("checksum decode failed: {err}")))?;
    let expected_hash = parse_sha256_file(&checksum_text)?;
    verify_sha256(&archive_bytes, &expected_hash)?;
    extract_binary_from_archive(&archive_bytes, &assets.archive_name)
}

pub fn pending_update_dir(aikit_dir: &Path) -> PathBuf {
    aikit_dir.join("pending-update")
}

pub fn pending_update_path(aikit_dir: &Path) -> PathBuf {
    pending_update_dir(aikit_dir).join(binary_file_name())
}

pub fn pending_update_version(aikit_dir: &Path) -> Result<Option<String>> {
    if !pending_update_path(aikit_dir).is_file() {
        return Ok(None);
    }

    let version = match fs::read_to_string(pending_update_dir(aikit_dir).join("version")) {
        Ok(version) => version,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(AikitError::Io(err)),
    };
    let version = version.trim();
    Ok((!version.is_empty()).then(|| version.to_string()))
}

pub fn clear_pending_update(aikit_dir: &Path) -> Result<()> {
    let dir = pending_update_dir(aikit_dir);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageUpdateOutcome {
    NoUpdate,
    AlreadyStaged { version: String },
    Staged { version: String },
}

pub async fn stage_update_if_available(
    client: &Client,
    latest_release_url: &str,
    aikit_dir: &Path,
    skipped_version: Option<&str>,
) -> Result<StageUpdateOutcome> {
    stage_update_with_progress(
        client,
        latest_release_url,
        aikit_dir,
        skipped_version,
        |_| {},
    )
    .await
}

pub async fn stage_update_with_progress(
    client: &Client,
    latest_release_url: &str,
    aikit_dir: &Path,
    skipped_version: Option<&str>,
    mut on_download: impl FnMut(&str),
) -> Result<StageUpdateOutcome> {
    let tag_name = fetch_latest_release_tag(client, latest_release_url).await?;
    let outcome = update_check_outcome(&tag_name)?;
    if !outcome.update_available {
        return Ok(StageUpdateOutcome::NoUpdate);
    }
    if skipped_version == Some(outcome.latest_version.as_str()) {
        return Ok(StageUpdateOutcome::NoUpdate);
    }

    let pending = pending_update_path(aikit_dir);
    if pending_update_version(aikit_dir)?.as_deref() == Some(outcome.latest_version.as_str()) {
        return Ok(StageUpdateOutcome::AlreadyStaged {
            version: outcome.latest_version,
        });
    }

    let assets = release_assets_for_tag(latest_release_url, tag_name)?;
    on_download(&outcome.latest_version);
    let staged = download_and_stage_assets(client, &assets).await?;
    let pending_dir = pending_update_dir(aikit_dir);
    fs::create_dir_all(&pending_dir)?;
    let mut version_file = tempfile::Builder::new()
        .prefix(".aikit-version-")
        .tempfile_in(&pending_dir)?;
    version_file.write_all(outcome.latest_version.as_bytes())?;
    version_file.as_file().sync_all()?;

    let installation =
        install_binary_with_backup(&staged.path().join(binary_file_name()), &pending)?;
    if let Err(err) = version_file.persist(pending_dir.join("version")) {
        if let Err(rollback_error) = installation.rollback() {
            return Err(AikitError::Provider(format!(
                "update version publication failed: {}; rollback failed: {rollback_error}",
                err.error
            )));
        }
        return Err(AikitError::Io(err.error));
    }

    Ok(StageUpdateOutcome::Staged {
        version: outcome.latest_version,
    })
}

async fn fetch_latest_release_tag(client: &Client, latest_release_url: &str) -> Result<String> {
    let response = send_get_with_retries(client, latest_release_url, "update request")
        .await?
        .error_for_status()
        .map_err(|err| AikitError::Provider(format!("update request failed: {err}")))?;
    parse_release_tag_from_url(response.url().as_str())
}

const UPDATE_REQUEST_ATTEMPTS: usize = 3;
const UPDATE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

/// Transient network failures and server-side errors are retried a few times;
/// deterministic client errors (4xx) are returned to the caller as-is.
async fn send_get_with_retries(
    client: &Client,
    url: &str,
    context: &str,
) -> Result<reqwest::Response> {
    let mut last_error = AikitError::Provider(format!("{context} failed"));
    for attempt in 1..=UPDATE_REQUEST_ATTEMPTS {
        if attempt > 1 {
            tokio::time::sleep(UPDATE_RETRY_DELAY * (attempt as u32 - 1)).await;
        }
        let request = client.get(url).header("User-Agent", "aikit").send().await;
        match request {
            Ok(response) => {
                let status = response.status();
                if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    last_error = AikitError::Provider(format!(
                        "{context} failed: server returned {status} (attempt {attempt}/{UPDATE_REQUEST_ATTEMPTS})"
                    ));
                    continue;
                }
                return Ok(response);
            }
            Err(err) => {
                last_error = AikitError::Provider(format!(
                    "{context} failed: {err} (attempt {attempt}/{UPDATE_REQUEST_ATTEMPTS})"
                ));
            }
        }
    }
    Err(last_error)
}

fn parse_release_tag_from_url(url: &str) -> Result<String> {
    let tag = url
        .split("/tag/")
        .last()
        .unwrap_or_default()
        .trim_matches('/');
    if tag.is_empty() {
        return Err(AikitError::Provider(format!(
            "could not determine latest release tag from {url}"
        )));
    }
    Ok(tag.to_string())
}

async fn download_bytes(client: &Client, url: &str) -> Result<Vec<u8>> {
    send_get_with_retries(client, url, "download")
        .await?
        .error_for_status()
        .map_err(|err| AikitError::Provider(format!("download failed: {err}")))?
        .bytes()
        .await
        .map_err(|err| AikitError::Provider(format!("download failed: {err}")))
        .map(|bytes| bytes.to_vec())
}

fn verify_sha256(bytes: &[u8], expected: &str) -> Result<()> {
    let digest = Sha256::digest(bytes);
    let actual = hex::encode(digest);
    if actual != expected {
        return Err(AikitError::Provider(format!(
            "sha256 mismatch: expected {expected}, got {actual}"
        )));
    }
    Ok(())
}

fn extract_binary_from_archive(bytes: &[u8], archive_name: &str) -> Result<tempfile::TempDir> {
    let extract_dir = tempfile::Builder::new().prefix("aikit-update-").tempdir()?;
    let binary_name = binary_file_name();

    if archive_name.ends_with(".zip") {
        extract_zip(bytes, extract_dir.path(), binary_name)?;
    } else if archive_name.ends_with(".tar.gz") {
        extract_tar_gz(bytes, extract_dir.path(), binary_name)?;
    } else {
        return Err(AikitError::Provider(format!(
            "unsupported archive format: {archive_name}"
        )));
    }

    let staged = extract_dir.path().join(binary_name);
    if !staged.exists() {
        return Err(AikitError::Provider(format!(
            "archive does not contain `{binary_name}`"
        )));
    }

    Ok(extract_dir)
}

fn extract_zip(bytes: &[u8], dest: &Path, binary_name: &str) -> Result<()> {
    let reader = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader)
        .map_err(|err| AikitError::Provider(format!("zip extract failed: {err}")))?;
    let mut entry = archive
        .by_name(binary_name)
        .map_err(|err| AikitError::Provider(format!("zip entry missing: {err}")))?;
    let out_path = dest.join(binary_name);
    let mut out_file = fs::File::create(&out_path)?;
    copy(&mut entry, &mut out_file)?;
    Ok(())
}

fn extract_tar_gz(bytes: &[u8], dest: &Path, binary_name: &str) -> Result<()> {
    let reader = GzDecoder::new(bytes);
    let mut archive = Archive::new(reader);
    for entry in archive
        .entries()
        .map_err(|err| AikitError::Provider(format!("tar extract failed: {err}")))?
    {
        let mut entry =
            entry.map_err(|err| AikitError::Provider(format!("tar extract failed: {err}")))?;
        let path = entry
            .path()
            .map_err(|err| AikitError::Provider(format!("tar extract failed: {err}")))?;
        if path.file_name().and_then(|name| name.to_str()) == Some(binary_name) {
            let out_path = dest.join(binary_name);
            let mut out_file = fs::File::create(&out_path)?;
            copy(&mut entry, &mut out_file)?;
            return Ok(());
        }
    }
    Err(AikitError::Provider(format!(
        "tar archive does not contain `{binary_name}`"
    )))
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    let width = left_parts.len().max(right_parts.len());
    for index in 0..width {
        let left_part = left_parts.get(index).copied().unwrap_or(0);
        let right_part = right_parts.get(index).copied().unwrap_or(0);
        match left_part.cmp(&right_part) {
            std::cmp::Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    std::cmp::Ordering::Equal
}

fn version_parts(version: &str) -> Vec<u64> {
    version
        .split(['.', '-'])
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}
