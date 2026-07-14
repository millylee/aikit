use std::io::Write;

use aikit_core::updater::{
    binary_file_name, check_for_updates, download_and_stage, parse_sha256_file,
    release_archive_name, update_check_cooldown_active, update_check_timestamp_now,
    version_is_newer,
};
use flate2::{write::GzEncoder, Compression};
use sha2::{Digest, Sha256};
use tar::Builder;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn archive_fixture() -> (Vec<u8>, String, String) {
    let archive_name = release_archive_name().unwrap();
    let binary_name = binary_file_name();
    let payload = b"updated-aikit-binary";

    let archive_bytes = if archive_name.ends_with(".zip") {
        let mut buffer = Vec::new();
        {
            let mut writer = ZipWriter::new(std::io::Cursor::new(&mut buffer));
            writer
                .start_file(
                    binary_name,
                    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(payload).unwrap();
            writer.finish().unwrap();
        }
        buffer
    } else {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut archive = Builder::new(&mut encoder);
            let mut header = tar::Header::new_gnu();
            header.set_size(payload.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            archive
                .append_data(&mut header, binary_name, &payload[..])
                .unwrap();
            archive.into_inner().unwrap();
        }
        encoder.finish().unwrap()
    };

    let digest = Sha256::digest(&archive_bytes);
    let checksum = format!("{checksum}  {archive_name}", checksum = hex::encode(digest));
    (archive_bytes, archive_name, checksum)
}

#[test]
fn version_is_newer_compares_semver_like_parts() {
    assert!(version_is_newer("1.0.1", "1.0.0"));
    assert!(!version_is_newer("1.0.0", "1.0.0"));
    assert!(!version_is_newer("0.9.9", "1.0.0"));
}

#[test]
fn parse_sha256_file_reads_release_checksum_format() {
    let parsed = parse_sha256_file("abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234  aikit-x86_64-pc-windows-msvc.zip\n").unwrap();
    assert_eq!(
        parsed,
        "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234"
    );
}

#[test]
fn update_check_cooldown_active_within_24_hours() {
    let recent = update_check_timestamp_now();
    assert!(update_check_cooldown_active(Some(recent.as_str())));
}

#[test]
fn update_check_cooldown_inactive_after_24_hours() {
    assert!(!update_check_cooldown_active(Some("2020-01-01T00:00:00Z")));
}

#[test]
fn update_check_cooldown_inactive_when_never_checked() {
    assert!(!update_check_cooldown_active(None));
}

#[tokio::test]
async fn check_for_updates_detects_newer_release() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/millylee/aikit/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tag_name": "v999.0.0",
            "assets": []
        })))
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    let outcome = check_for_updates(
        &client,
        &format!("{}/repos/millylee/aikit/releases/latest", server.uri()),
    )
    .await
    .unwrap();

    assert!(outcome.update_available);
    assert_eq!(outcome.latest_version, "999.0.0");
}

#[tokio::test]
async fn download_and_stage_verifies_checksum_and_extracts_binary() {
    let server = MockServer::start().await;
    let (archive_bytes, archive_name, checksum) = archive_fixture();

    Mock::given(method("GET"))
        .and(path("/repos/millylee/aikit/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tag_name": "v2.0.0",
            "assets": [
                {
                    "name": archive_name,
                    "browser_download_url": format!("{}/archive", server.uri())
                },
                {
                    "name": format!("{archive_name}.sha256"),
                    "browser_download_url": format!("{}/checksum", server.uri())
                }
            ]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/archive"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(archive_bytes))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/checksum"))
        .respond_with(ResponseTemplate::new(200).set_body_string(checksum))
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    let staged = download_and_stage(
        &client,
        &format!("{}/repos/millylee/aikit/releases/latest", server.uri()),
    )
    .await
    .unwrap();

    assert!(staged.exists());
    assert_eq!(
        staged.file_name().and_then(|name| name.to_str()),
        Some(binary_file_name())
    );
    assert_eq!(std::fs::read(staged).unwrap(), b"updated-aikit-binary");
}
