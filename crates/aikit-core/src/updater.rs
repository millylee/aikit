use std::{
    fs,
    io::copy,
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tar::Archive;
use zip::ZipArchive;

use crate::{AikitError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheckOutcome {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateApplyOutcome {
    pub message: String,
    pub quit_after: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAssets {
    pub tag_name: String,
    pub archive_name: String,
    pub archive_url: String,
    pub checksum_name: String,
    pub checksum_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubLatestRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
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
    let release = fetch_latest_release(client, latest_release_url).await?;
    let latest_version = normalize_release_tag(&release.tag_name);
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
    let release = fetch_latest_release(client, latest_release_url).await?;
    let archive_name = release_archive_name()?;
    let checksum_name = format!("{archive_name}.sha256");

    let archive_url = release
        .assets
        .iter()
        .find(|asset| asset.name == archive_name)
        .map(|asset| asset.browser_download_url.clone())
        .ok_or_else(|| {
            AikitError::Provider(format!("release does not include asset `{archive_name}`"))
        })?;

    let checksum_url = release
        .assets
        .iter()
        .find(|asset| asset.name == checksum_name)
        .map(|asset| asset.browser_download_url.clone())
        .ok_or_else(|| {
            AikitError::Provider(format!("release does not include asset `{checksum_name}`"))
        })?;

    Ok(ReleaseAssets {
        tag_name: release.tag_name,
        archive_name,
        archive_url,
        checksum_name,
        checksum_url,
    })
}

pub async fn download_and_stage(client: &Client, latest_release_url: &str) -> Result<PathBuf> {
    let assets = fetch_release_assets(client, latest_release_url).await?;
    let archive_bytes = download_bytes(client, &assets.archive_url).await?;
    let checksum_bytes = download_bytes(client, &assets.checksum_url).await?;
    let checksum_text = String::from_utf8(checksum_bytes)
        .map_err(|err| AikitError::Provider(format!("checksum decode failed: {err}")))?;
    let expected_hash = parse_sha256_file(&checksum_text)?;
    verify_sha256(&archive_bytes, &expected_hash)?;
    extract_binary_from_archive(&archive_bytes, &assets.archive_name)
}

pub fn pending_update_path(aikit_dir: &Path) -> PathBuf {
    aikit_dir.join("pending-update").join(binary_file_name())
}

pub fn clear_pending_update(aikit_dir: &Path) -> Result<()> {
    let dir = aikit_dir.join("pending-update");
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
    let outcome = check_for_updates(client, latest_release_url).await?;
    if !outcome.update_available {
        return Ok(StageUpdateOutcome::NoUpdate);
    }
    if skipped_version == Some(outcome.latest_version.as_str()) {
        return Ok(StageUpdateOutcome::NoUpdate);
    }

    let pending = pending_update_path(aikit_dir);
    if pending.exists() {
        return Ok(StageUpdateOutcome::AlreadyStaged {
            version: outcome.latest_version,
        });
    }

    let staged = download_and_stage(client, latest_release_url).await?;
    if let Some(parent) = pending.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&staged, &pending)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&pending)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&pending, permissions)?;
    }

    Ok(StageUpdateOutcome::Staged {
        version: outcome.latest_version,
    })
}

pub fn apply_pending_update_at_startup(
    aikit_dir: &Path,
    #[allow(unused_variables)] pending_version: Option<&str>,
) -> Result<Option<String>> {
    let pending = pending_update_path(aikit_dir);
    if !pending.exists() {
        return Ok(None);
    }

    let target = std::env::current_exe().map_err(AikitError::Io)?;

    #[cfg(windows)]
    {
        spawn_windows_replacer_and_launch(&pending, &target, aikit_dir)?;
        std::process::exit(0);
    }

    #[cfg(not(windows))]
    {
        install_binary(&pending, &target)?;
        clear_pending_update(aikit_dir)?;
        Ok(pending_version.map(str::to_string))
    }
}

pub async fn perform_update(
    client: &Client,
    latest_release_url: &str,
) -> Result<UpdateApplyOutcome> {
    let staged = download_and_stage(client, latest_release_url).await?;
    let target = std::env::current_exe().map_err(AikitError::Io)?;

    #[cfg(windows)]
    {
        spawn_windows_replacer(&staged, &target)?;
        Ok(UpdateApplyOutcome {
            message: "Update scheduled. Please restart aikit.".into(),
            quit_after: true,
        })
    }

    #[cfg(not(windows))]
    {
        install_binary(&staged, &target)?;
        Ok(UpdateApplyOutcome {
            message: "Update installed. Please restart aikit.".into(),
            quit_after: true,
        })
    }
}

pub fn install_binary(staged: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(staged, target)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(target)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(target, permissions)?;
    }
    Ok(())
}

#[cfg(windows)]
pub fn spawn_windows_replacer_and_launch(
    staged: &Path,
    target: &Path,
    aikit_dir: &Path,
) -> Result<()> {
    use std::process::Command;

    let staged = powershell_literal(staged);
    let target = powershell_literal(target);
    let pending_dir = powershell_literal(&aikit_dir.join("pending-update"));
    let script = format!(
        "Start-Sleep -Seconds 1; Copy-Item -LiteralPath '{staged}' -Destination '{target}' -Force; Remove-Item -LiteralPath '{pending_dir}' -Recurse -Force; Start-Process -FilePath '{target}'"
    );
    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
        .spawn()
        .map_err(AikitError::Io)?;
    Ok(())
}

#[cfg(not(windows))]
pub fn spawn_windows_replacer_and_launch(
    _staged: &Path,
    _target: &Path,
    _aikit_dir: &Path,
) -> Result<()> {
    Err(AikitError::Provider(
        "windows updater helper is only available on windows".into(),
    ))
}

#[cfg(windows)]
pub fn spawn_windows_replacer(staged: &Path, target: &Path) -> Result<()> {
    use std::process::Command;

    let staged = powershell_literal(staged);
    let target = powershell_literal(target);
    let script = format!(
        "Start-Sleep -Seconds 1; Copy-Item -LiteralPath '{staged}' -Destination '{target}' -Force"
    );
    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
        .spawn()
        .map_err(AikitError::Io)?;
    Ok(())
}

#[cfg(not(windows))]
pub fn spawn_windows_replacer(_staged: &Path, _target: &Path) -> Result<()> {
    Err(AikitError::Provider(
        "windows updater helper is only available on windows".into(),
    ))
}

async fn fetch_latest_release(
    client: &Client,
    latest_release_url: &str,
) -> Result<GithubLatestRelease> {
    client
        .get(latest_release_url)
        .header("User-Agent", "aikit")
        .send()
        .await
        .map_err(|err| AikitError::Provider(format!("update request failed: {err}")))?
        .error_for_status()
        .map_err(|err| AikitError::Provider(format!("update request failed: {err}")))?
        .json::<GithubLatestRelease>()
        .await
        .map_err(|err| AikitError::Provider(format!("update response parse failed: {err}")))
}

async fn download_bytes(client: &Client, url: &str) -> Result<Vec<u8>> {
    client
        .get(url)
        .header("User-Agent", "aikit")
        .send()
        .await
        .map_err(|err| AikitError::Provider(format!("download failed: {err}")))?
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

fn extract_binary_from_archive(bytes: &[u8], archive_name: &str) -> Result<PathBuf> {
    let extract_dir =
        std::env::temp_dir().join(format!("aikit-update-extract-{}", std::process::id()));
    fs::create_dir_all(&extract_dir)?;
    let binary_name = binary_file_name();

    if archive_name.ends_with(".zip") {
        extract_zip(bytes, &extract_dir, binary_name)?;
    } else if archive_name.ends_with(".tar.gz") {
        extract_tar_gz(bytes, &extract_dir, binary_name)?;
    } else {
        return Err(AikitError::Provider(format!(
            "unsupported archive format: {archive_name}"
        )));
    }

    let staged = extract_dir.join(binary_name);
    if !staged.exists() {
        return Err(AikitError::Provider(format!(
            "archive does not contain `{binary_name}`"
        )));
    }

    let persistent_dir = std::env::temp_dir().join(format!("aikit-update-{}", std::process::id()));
    fs::create_dir_all(&persistent_dir)?;
    let persistent_path = persistent_dir.join(binary_name);
    fs::copy(&staged, &persistent_path)?;
    Ok(persistent_path)
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

#[cfg(windows)]
fn powershell_literal(path: &Path) -> String {
    path.display().to_string().replace('\'', "''")
}
