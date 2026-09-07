use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use aikit_core::updater::{
    binary_file_name, check_for_updates, download_and_stage, parse_sha256_file,
    pending_update_path, pending_update_version, release_archive_name, stage_update_if_available,
    stage_update_with_progress, update_check_cooldown_active, update_check_timestamp_now,
    version_is_newer, StageUpdateOutcome,
};
use flate2::{write::GzEncoder, Compression};
use sha2::{Digest, Sha256};
use tar::Builder;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, Request, ResponseTemplate,
};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn archive_fixture() -> (Vec<u8>, String, String) {
    archive_fixture_with_payload(b"updated-aikit-binary")
}

fn archive_fixture_with_payload(payload: &[u8]) -> (Vec<u8>, String, String) {
    let archive_name = release_archive_name().unwrap();
    let binary_name = binary_file_name();

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
                .append_data(&mut header, binary_name, payload)
                .unwrap();
            archive.into_inner().unwrap();
        }
        encoder.finish().unwrap()
    };

    let digest = Sha256::digest(&archive_bytes);
    let checksum = format!("{checksum}  {archive_name}", checksum = hex::encode(digest));
    (archive_bytes, archive_name, checksum)
}

async fn mock_latest_release(server: &MockServer, tag: &str) -> String {
    Mock::given(method("GET"))
        .and(path("/millylee/aikit/releases/latest"))
        .respond_with(ResponseTemplate::new(302).insert_header(
            "Location",
            format!("{}/millylee/aikit/releases/tag/{tag}", server.uri()),
        ))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/millylee/aikit/releases/tag/{tag}")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(server)
        .await;
    format!("{}/millylee/aikit/releases/latest", server.uri())
}

async fn mock_release_asset(
    server: &MockServer,
    tag: &str,
    name: &str,
    response: ResponseTemplate,
) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/millylee/aikit/releases/download/{tag}/{name}"
        )))
        .respond_with(response)
        .expect(1)
        .mount(server)
        .await;
}

fn seed_pending_candidate(aikit_dir: &Path, version: Option<&str>) -> PathBuf {
    let pending = pending_update_path(aikit_dir);
    fs::create_dir_all(pending.parent().unwrap()).unwrap();
    fs::write(&pending, b"previous-aikit-binary").unwrap();
    if let Some(version) = version {
        fs::write(aikit_dir.join("pending-update/version"), version).unwrap();
    }
    pending
}

fn assert_pending_candidate_unchanged(aikit_dir: &Path, version: Option<&str>) {
    assert_eq!(
        fs::read(pending_update_path(aikit_dir)).unwrap(),
        b"previous-aikit-binary"
    );
    let version_path = aikit_dir.join("pending-update/version");
    match version {
        Some(version) => assert_eq!(fs::read_to_string(version_path).unwrap(), version),
        None => assert!(!version_path.exists()),
    }
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
        .and(path("/millylee/aikit/releases/latest"))
        .respond_with(ResponseTemplate::new(302).insert_header(
            "Location",
            format!("{}/millylee/aikit/releases/tag/v999.0.0", server.uri()),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/millylee/aikit/releases/tag/v999.0.0"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    let outcome = check_for_updates(
        &client,
        &format!("{}/millylee/aikit/releases/latest", server.uri()),
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
        .and(path("/millylee/aikit/releases/latest"))
        .respond_with(ResponseTemplate::new(302).insert_header(
            "Location",
            format!("{}/millylee/aikit/releases/tag/v2.0.0", server.uri()),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/millylee/aikit/releases/tag/v2.0.0"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/millylee/aikit/releases/download/v2.0.0/{archive_name}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(archive_bytes))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/millylee/aikit/releases/download/v2.0.0/{archive_name}.sha256"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(checksum))
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    let staged = download_and_stage(
        &client,
        &format!("{}/millylee/aikit/releases/latest", server.uri()),
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

#[tokio::test]
async fn stage_update_reports_progress_before_delayed_archive_response() {
    let server = MockServer::start().await;
    let latest_url = mock_latest_release(&server, "v999.0.0").await;
    let (archive_bytes, archive_name, checksum) = archive_fixture();
    let archive_requested = Arc::new(AtomicBool::new(false));
    let request_observer = Arc::clone(&archive_requested);
    Mock::given(method("GET"))
        .and(path(format!(
            "/millylee/aikit/releases/download/v999.0.0/{archive_name}"
        )))
        .respond_with(move |_: &Request| {
            request_observer.store(true, Ordering::SeqCst);
            ResponseTemplate::new(200)
                .set_body_bytes(archive_bytes.clone())
                .set_delay(Duration::from_millis(250))
        })
        .expect(1)
        .mount(&server)
        .await;
    mock_release_asset(
        &server,
        "v999.0.0",
        &format!("{archive_name}.sha256"),
        ResponseTemplate::new(200).set_body_string(checksum),
    )
    .await;

    let aikit_dir = tempfile::tempdir().unwrap();
    let client = reqwest::Client::new();
    let (progress_sender, mut progress_receiver) = tokio::sync::mpsc::unbounded_channel();
    let staging =
        stage_update_with_progress(&client, &latest_url, aikit_dir.path(), None, |version| {
            assert!(!archive_requested.load(Ordering::SeqCst));
            progress_sender.send(version.to_string()).unwrap();
        });
    tokio::pin!(staging);

    let version = tokio::select! {
        biased;
        version = progress_receiver.recv() => version.unwrap(),
        outcome = &mut staging => panic!("staging completed before progress: {outcome:?}"),
    };
    assert_eq!(version, "999.0.0");
    assert!(!pending_update_path(aikit_dir.path()).exists());
    assert_eq!(
        staging.await.unwrap(),
        StageUpdateOutcome::Staged {
            version: "999.0.0".into()
        }
    );
    assert!(progress_receiver.try_recv().is_err());
}

#[tokio::test]
async fn consecutive_downloads_keep_independent_staged_candidates() {
    let mut candidates = Vec::new();
    let payloads = [
        b"first candidate".as_slice(),
        b"second candidate".as_slice(),
    ];
    for payload in payloads {
        let server = MockServer::start().await;
        let latest_url = mock_latest_release(&server, "v999.0.0").await;
        let (archive_bytes, archive_name, checksum) = archive_fixture_with_payload(payload);
        mock_release_asset(
            &server,
            "v999.0.0",
            &archive_name,
            ResponseTemplate::new(200).set_body_bytes(archive_bytes),
        )
        .await;
        mock_release_asset(
            &server,
            "v999.0.0",
            &format!("{archive_name}.sha256"),
            ResponseTemplate::new(200).set_body_string(checksum),
        )
        .await;
        candidates.push(
            download_and_stage(&reqwest::Client::new(), &latest_url)
                .await
                .unwrap(),
        );
    }

    assert_ne!(candidates[0], candidates[1]);
    for (candidate, payload) in candidates.iter().zip(payloads) {
        assert_eq!(fs::read(candidate).unwrap(), payload);
    }
}

#[tokio::test]
async fn stage_update_resolves_latest_once_and_persists_version() {
    let server = MockServer::start().await;
    let latest_url = mock_latest_release(&server, "999.0.0").await;
    let (archive_bytes, archive_name, checksum) = archive_fixture();
    mock_release_asset(
        &server,
        "999.0.0",
        &archive_name,
        ResponseTemplate::new(200).set_body_bytes(archive_bytes),
    )
    .await;
    mock_release_asset(
        &server,
        "999.0.0",
        &format!("{archive_name}.sha256"),
        ResponseTemplate::new(200).set_body_string(checksum),
    )
    .await;

    let aikit_dir = tempfile::tempdir().unwrap();
    let outcome =
        stage_update_if_available(&reqwest::Client::new(), &latest_url, aikit_dir.path(), None)
            .await
            .unwrap();

    server.verify().await;
    assert_eq!(
        outcome,
        StageUpdateOutcome::Staged {
            version: "999.0.0".into()
        }
    );
    assert_eq!(
        fs::read(pending_update_path(aikit_dir.path())).unwrap(),
        b"updated-aikit-binary"
    );
    assert_eq!(
        fs::read_to_string(aikit_dir.path().join("pending-update/version")).unwrap(),
        "999.0.0"
    );
    assert_eq!(
        pending_update_version(aikit_dir.path()).unwrap().as_deref(),
        Some("999.0.0")
    );
}

#[tokio::test]
async fn stage_update_replaces_old_or_unversioned_pending_candidate() {
    for previous_version in [None, Some("998.0.0")] {
        let server = MockServer::start().await;
        let latest_url = mock_latest_release(&server, "v999.0.0").await;
        let (archive_bytes, archive_name, checksum) = archive_fixture();
        mock_release_asset(
            &server,
            "v999.0.0",
            &archive_name,
            ResponseTemplate::new(200).set_body_bytes(archive_bytes),
        )
        .await;
        mock_release_asset(
            &server,
            "v999.0.0",
            &format!("{archive_name}.sha256"),
            ResponseTemplate::new(200).set_body_string(checksum),
        )
        .await;
        let aikit_dir = tempfile::tempdir().unwrap();
        let pending = seed_pending_candidate(aikit_dir.path(), previous_version);
        let mut progress = Vec::new();

        let outcome = stage_update_with_progress(
            &reqwest::Client::new(),
            &latest_url,
            aikit_dir.path(),
            None,
            |version| {
                assert_pending_candidate_unchanged(aikit_dir.path(), previous_version);
                progress.push(version.to_string());
            },
        )
        .await
        .unwrap();

        assert_eq!(
            outcome,
            StageUpdateOutcome::Staged {
                version: "999.0.0".into()
            }
        );
        assert_eq!(progress, ["999.0.0"]);
        assert_eq!(fs::read(pending).unwrap(), b"updated-aikit-binary");
        assert_eq!(
            pending_update_version(aikit_dir.path()).unwrap().as_deref(),
            Some("999.0.0")
        );
    }
}

#[tokio::test]
async fn stage_update_preserves_pending_candidate_on_version_publication_failure() {
    let server = MockServer::start().await;
    let latest_url = mock_latest_release(&server, "v999.0.0").await;
    let (archive_bytes, archive_name, checksum) = archive_fixture();
    mock_release_asset(
        &server,
        "v999.0.0",
        &archive_name,
        ResponseTemplate::new(200).set_body_bytes(archive_bytes),
    )
    .await;
    mock_release_asset(
        &server,
        "v999.0.0",
        &format!("{archive_name}.sha256"),
        ResponseTemplate::new(200).set_body_string(checksum),
    )
    .await;
    let aikit_dir = tempfile::tempdir().unwrap();
    let pending = seed_pending_candidate(aikit_dir.path(), None);
    let version_path = aikit_dir.path().join("pending-update/version");

    let error = stage_update_with_progress(
        &reqwest::Client::new(),
        &latest_url,
        aikit_dir.path(),
        None,
        |version| {
            assert_eq!(version, "999.0.0");
            fs::create_dir(&version_path).unwrap();
            fs::write(version_path.join("keep.txt"), b"keep").unwrap();
        },
    )
    .await
    .unwrap_err();

    assert!(matches!(error, aikit_core::AikitError::Io(_)));
    assert_eq!(fs::read(&pending).unwrap(), b"previous-aikit-binary");
    assert_eq!(fs::read(version_path.join("keep.txt")).unwrap(), b"keep");
    aikit_core::updater::cleanup_previous_binary(&pending).unwrap();
    server.verify().await;
}

#[tokio::test]
async fn stage_update_removes_new_candidate_on_version_publication_failure() {
    let server = MockServer::start().await;
    let latest_url = mock_latest_release(&server, "v999.0.0").await;
    let (archive_bytes, archive_name, checksum) = archive_fixture();
    mock_release_asset(
        &server,
        "v999.0.0",
        &archive_name,
        ResponseTemplate::new(200).set_body_bytes(archive_bytes),
    )
    .await;
    mock_release_asset(
        &server,
        "v999.0.0",
        &format!("{archive_name}.sha256"),
        ResponseTemplate::new(200).set_body_string(checksum),
    )
    .await;
    let aikit_dir = tempfile::tempdir().unwrap();
    let version_path = aikit_dir.path().join("pending-update/version");

    let error = stage_update_with_progress(
        &reqwest::Client::new(),
        &latest_url,
        aikit_dir.path(),
        None,
        |_| fs::create_dir_all(&version_path).unwrap(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, aikit_core::AikitError::Io(_)));
    let pending = pending_update_path(aikit_dir.path());
    assert!(!pending.exists());
    assert!(version_path.is_dir());
    aikit_core::updater::cleanup_previous_binary(&pending).unwrap();
    assert_eq!(
        fs::read_dir(version_path.parent().unwrap())
            .unwrap()
            .count(),
        2
    );
    server.verify().await;
}

#[cfg(windows)]
#[tokio::test]
async fn stage_update_preserves_pending_version_when_metadata_is_locked() {
    use std::os::windows::fs::OpenOptionsExt;

    let server = MockServer::start().await;
    let latest_url = mock_latest_release(&server, "v999.0.0").await;
    let (archive_bytes, archive_name, checksum) = archive_fixture();
    mock_release_asset(
        &server,
        "v999.0.0",
        &archive_name,
        ResponseTemplate::new(200).set_body_bytes(archive_bytes),
    )
    .await;
    mock_release_asset(
        &server,
        "v999.0.0",
        &format!("{archive_name}.sha256"),
        ResponseTemplate::new(200).set_body_string(checksum),
    )
    .await;
    let aikit_dir = tempfile::tempdir().unwrap();
    let pending = seed_pending_candidate(aikit_dir.path(), Some("998.0.0\n"));
    let version_path = aikit_dir.path().join("pending-update/version");
    let version_guard = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x0000_0001)
        .open(&version_path)
        .unwrap();

    let error =
        stage_update_if_available(&reqwest::Client::new(), &latest_url, aikit_dir.path(), None)
            .await
            .unwrap_err();

    assert!(matches!(error, aikit_core::AikitError::Io(_)));
    assert_pending_candidate_unchanged(aikit_dir.path(), Some("998.0.0\n"));
    drop(version_guard);
    aikit_core::updater::cleanup_previous_binary(&pending).unwrap();
    assert_eq!(
        fs::read_dir(version_path.parent().unwrap())
            .unwrap()
            .count(),
        3
    );
    server.verify().await;
}

#[tokio::test]
async fn stage_update_preserves_pending_candidate_on_checksum_mismatch() {
    for previous_version in [None, Some("998.0.0")] {
        let server = MockServer::start().await;
        let latest_url = mock_latest_release(&server, "v999.0.0").await;
        let (archive_bytes, archive_name, _) = archive_fixture();
        mock_release_asset(
            &server,
            "v999.0.0",
            &archive_name,
            ResponseTemplate::new(200).set_body_bytes(archive_bytes),
        )
        .await;
        mock_release_asset(
            &server,
            "v999.0.0",
            &format!("{archive_name}.sha256"),
            ResponseTemplate::new(200).set_body_string("0".repeat(64)),
        )
        .await;
        let aikit_dir = tempfile::tempdir().unwrap();
        seed_pending_candidate(aikit_dir.path(), previous_version);

        let error =
            stage_update_if_available(&reqwest::Client::new(), &latest_url, aikit_dir.path(), None)
                .await
                .unwrap_err();

        assert!(error.to_string().contains("sha256 mismatch"));
        assert_pending_candidate_unchanged(aikit_dir.path(), previous_version);
    }
}

#[tokio::test]
async fn stage_update_preserves_pending_candidate_on_download_failure() {
    for previous_version in [None, Some("998.0.0")] {
        let server = MockServer::start().await;
        let latest_url = mock_latest_release(&server, "v999.0.0").await;
        mock_release_asset(
            &server,
            "v999.0.0",
            &release_archive_name().unwrap(),
            ResponseTemplate::new(503),
        )
        .await;
        let aikit_dir = tempfile::tempdir().unwrap();
        seed_pending_candidate(aikit_dir.path(), previous_version);

        let error =
            stage_update_if_available(&reqwest::Client::new(), &latest_url, aikit_dir.path(), None)
                .await
                .unwrap_err();

        assert!(error.to_string().contains("download failed"));
        assert_pending_candidate_unchanged(aikit_dir.path(), previous_version);
    }
}

#[tokio::test]
async fn stage_update_reuses_only_matching_pending_version_without_progress() {
    let server = MockServer::start().await;
    let latest_url = mock_latest_release(&server, "v999.0.0").await;
    let aikit_dir = tempfile::tempdir().unwrap();
    seed_pending_candidate(aikit_dir.path(), Some("999.0.0"));
    let mut progress = Vec::new();

    let outcome = stage_update_with_progress(
        &reqwest::Client::new(),
        &latest_url,
        aikit_dir.path(),
        None,
        |version| progress.push(version.to_string()),
    )
    .await
    .unwrap();

    assert_eq!(
        outcome,
        StageUpdateOutcome::AlreadyStaged {
            version: "999.0.0".into()
        }
    );
    assert!(progress.is_empty());
    assert_pending_candidate_unchanged(aikit_dir.path(), Some("999.0.0"));
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn stage_update_preserves_pending_candidate_on_checksum_download_or_parse_failure() {
    for (response, expected_error) in [
        (ResponseTemplate::new(503), "download failed"),
        (
            ResponseTemplate::new(200).set_body_string("invalid checksum"),
            "invalid sha256 checksum",
        ),
        (
            ResponseTemplate::new(200).set_body_bytes(vec![0xff]),
            "checksum decode failed",
        ),
    ] {
        let server = MockServer::start().await;
        let latest_url = mock_latest_release(&server, "v999.0.0").await;
        let (archive_bytes, archive_name, _) = archive_fixture();
        mock_release_asset(
            &server,
            "v999.0.0",
            &archive_name,
            ResponseTemplate::new(200).set_body_bytes(archive_bytes),
        )
        .await;
        mock_release_asset(
            &server,
            "v999.0.0",
            &format!("{archive_name}.sha256"),
            response,
        )
        .await;
        let aikit_dir = tempfile::tempdir().unwrap();
        seed_pending_candidate(aikit_dir.path(), Some("998.0.0"));

        let error =
            stage_update_if_available(&reqwest::Client::new(), &latest_url, aikit_dir.path(), None)
                .await
                .unwrap_err();

        assert!(error.to_string().contains(expected_error));
        assert_pending_candidate_unchanged(aikit_dir.path(), Some("998.0.0"));
    }
}

#[tokio::test]
async fn stage_update_does_not_report_downloading_for_current_or_skipped_release() {
    let current_tag = format!("v{}", env!("CARGO_PKG_VERSION"));
    for (tag, skipped_version) in [(current_tag.as_str(), None), ("v999.0.0", Some("999.0.0"))] {
        let server = MockServer::start().await;
        let latest_url = mock_latest_release(&server, tag).await;
        let aikit_dir = tempfile::tempdir().unwrap();
        seed_pending_candidate(aikit_dir.path(), Some("998.0.0"));
        let mut progress = Vec::new();

        let outcome = stage_update_with_progress(
            &reqwest::Client::new(),
            &latest_url,
            aikit_dir.path(),
            skipped_version,
            |version| progress.push(version.to_string()),
        )
        .await
        .unwrap();

        assert_eq!(outcome, StageUpdateOutcome::NoUpdate);
        assert!(progress.is_empty());
        assert_pending_candidate_unchanged(aikit_dir.path(), Some("998.0.0"));
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }
}

#[test]
fn pending_update_version_requires_candidate_and_nonempty_metadata() {
    let aikit_dir = tempfile::tempdir().unwrap();
    let version_path = aikit_dir.path().join("pending-update/version");
    assert_eq!(pending_update_version(aikit_dir.path()).unwrap(), None);

    fs::create_dir_all(version_path.parent().unwrap()).unwrap();
    fs::write(&version_path, "999.0.0").unwrap();
    assert_eq!(pending_update_version(aikit_dir.path()).unwrap(), None);

    seed_pending_candidate(aikit_dir.path(), Some(" 999.0.0\n"));
    assert_eq!(
        pending_update_version(aikit_dir.path()).unwrap().as_deref(),
        Some("999.0.0")
    );

    fs::write(&version_path, " \n").unwrap();
    assert_eq!(pending_update_version(aikit_dir.path()).unwrap(), None);

    fs::remove_file(version_path).unwrap();
    assert_eq!(pending_update_version(aikit_dir.path()).unwrap(), None);

    let pending = pending_update_path(aikit_dir.path());
    fs::remove_file(&pending).unwrap();
    fs::create_dir(&pending).unwrap();
    fs::write(aikit_dir.path().join("pending-update/version"), "999.0.0").unwrap();
    assert_eq!(pending_update_version(aikit_dir.path()).unwrap(), None);
}

#[test]
fn pending_update_version_propagates_metadata_read_errors() {
    let aikit_dir = tempfile::tempdir().unwrap();
    seed_pending_candidate(aikit_dir.path(), None);
    fs::create_dir(aikit_dir.path().join("pending-update/version")).unwrap();

    assert!(matches!(
        pending_update_version(aikit_dir.path()),
        Err(aikit_core::AikitError::Io(_))
    ));
}
