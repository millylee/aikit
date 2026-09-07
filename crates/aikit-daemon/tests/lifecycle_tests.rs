use std::{
    net::IpAddr,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

use aikit_daemon::lifecycle::{
    ensure_startable, format_http_url, read_daemon_info, remove_daemon_info, start_with_executable,
    start_with_executable_owned, status, stop, stop_owned_child, write_daemon_info, DaemonInfo,
    StartOutcome, StatusReport, StopOutcome,
};

fn sample_info() -> DaemonInfo {
    DaemonInfo {
        pid: 4242,
        port: 0,
        bind: "127.0.0.1".parse::<IpAddr>().unwrap(),
    }
}

#[tokio::test]
async fn daemon_info_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_daemon_info(dir.path()).is_none());

    write_daemon_info(dir.path(), &sample_info()).unwrap();
    assert_eq!(read_daemon_info(dir.path()), Some(sample_info()));

    remove_daemon_info(dir.path());
    assert!(read_daemon_info(dir.path()).is_none());
}

#[test]
fn format_http_url_brackets_ipv6() {
    let v4: IpAddr = "127.0.0.1".parse().unwrap();
    let v6: IpAddr = "::1".parse().unwrap();
    assert_eq!(format_http_url(v4, 7654), "http://127.0.0.1:7654");
    assert_eq!(format_http_url(v6, 7654), "http://[::1]:7654");
}

#[tokio::test]
async fn status_reports_stopped_without_state() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(status(dir.path()).await.unwrap(), StatusReport::Stopped);
}

#[tokio::test]
async fn status_reports_stale_when_probe_fails() {
    let dir = tempfile::tempdir().unwrap();
    write_daemon_info(dir.path(), &sample_info()).unwrap();

    assert_eq!(
        status(dir.path()).await.unwrap(),
        StatusReport::Stale {
            info: sample_info()
        }
    );
}

#[tokio::test]
async fn stop_is_idempotent_without_state() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(stop(dir.path()).await.unwrap(), StopOutcome::NotRunning);
}

#[tokio::test]
async fn stop_cleans_up_stale_state() {
    let dir = tempfile::tempdir().unwrap();
    write_daemon_info(dir.path(), &sample_info()).unwrap();

    assert_eq!(stop(dir.path()).await.unwrap(), StopOutcome::StaleRemoved);
    assert!(read_daemon_info(dir.path()).is_none());
}

#[tokio::test]
async fn ensure_startable_allows_when_no_state() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(ensure_startable(dir.path()).await.unwrap(), None);
}

#[tokio::test]
async fn ensure_startable_reclaims_stale_state() {
    let dir = tempfile::tempdir().unwrap();
    write_daemon_info(dir.path(), &sample_info()).unwrap();

    assert_eq!(ensure_startable(dir.path()).await.unwrap(), None);
    assert!(read_daemon_info(dir.path()).is_none());
}

#[tokio::test]
async fn daemon_start_uses_the_explicit_executable() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("missing-updated-aikit.exe");

    let error = start_with_executable(
        directory.path(),
        "127.0.0.1".parse().unwrap(),
        0,
        &executable,
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("failed to spawn daemon"));
    assert!(read_daemon_info(directory.path()).is_none());
}

#[tokio::test]
async fn owned_daemon_start_uses_the_explicit_executable() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("missing-updated-aikit.exe");

    let error = start_with_executable_owned(
        directory.path(),
        "127.0.0.1".parse().unwrap(),
        0,
        &executable,
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("failed to spawn daemon"));
    assert!(read_daemon_info(directory.path()).is_none());
}

#[tokio::test]
async fn owned_start_returns_no_child_for_an_existing_daemon() {
    let directory = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = axum::Router::new().route("/api/health", axum::routing::get(|| async { "ok" }));
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let info = DaemonInfo {
        pid: std::process::id(),
        port: address.port(),
        bind: address.ip(),
    };
    write_daemon_info(directory.path(), &info).unwrap();
    let executable = directory.path().join("must-not-be-spawned.exe");

    let (outcome, child) =
        start_with_executable_owned(directory.path(), address.ip(), address.port(), &executable)
            .await
            .unwrap();

    assert_eq!(outcome, StartOutcome::AlreadyRunning { info: info.clone() });
    assert!(child.is_none());
    assert_eq!(
        start_with_executable(directory.path(), address.ip(), address.port(), &executable)
            .await
            .unwrap(),
        outcome
    );
    assert_eq!(read_daemon_info(directory.path()), Some(info));
    server.abort();
}

struct TestChild {
    process: Child,
}

impl Drop for TestChild {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

fn spawn_test_child(directory: &Path) -> TestChild {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "owned_lifecycle_test_child"])
        .env("AIKIT_LIFECYCLE_OWNED_TEST_CHILD", "1")
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    TestChild {
        process: command.spawn().unwrap(),
    }
}

#[test]
fn owned_lifecycle_test_child() {
    if std::env::var_os("AIKIT_LIFECYCLE_OWNED_TEST_CHILD").is_some() {
        std::thread::sleep(Duration::from_secs(60));
    }
}

#[tokio::test]
async fn stop_owned_child_terminates_and_reaps_unhealthy_child() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = spawn_test_child(directory.path());
    let info = DaemonInfo {
        pid: child.process.id(),
        ..sample_info()
    };
    write_daemon_info(directory.path(), &info).unwrap();
    assert!(child.process.try_wait().unwrap().is_none());
    assert_eq!(
        status(directory.path()).await.unwrap(),
        StatusReport::Stale { info }
    );

    stop_owned_child(directory.path(), &mut child.process).unwrap();

    let exit_status = child
        .process
        .try_wait()
        .unwrap()
        .expect("owned unhealthy child must be terminated and reaped");
    assert!(!exit_status.success());
    assert_eq!(child.process.wait().unwrap(), exit_status);
    assert!(read_daemon_info(directory.path()).is_none());
}

#[test]
fn stop_owned_child_preserves_other_daemon_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = spawn_test_child(directory.path());
    let mut other_child = spawn_test_child(directory.path());
    let other_info = DaemonInfo {
        pid: other_child.process.id(),
        ..sample_info()
    };
    write_daemon_info(directory.path(), &other_info).unwrap();

    stop_owned_child(directory.path(), &mut child.process).unwrap();

    assert_eq!(read_daemon_info(directory.path()), Some(other_info));
    assert!(other_child.process.try_wait().unwrap().is_none());
    let exit_status = child
        .process
        .try_wait()
        .unwrap()
        .expect("only the owned child must be terminated and reaped");
    assert!(!exit_status.success());
    assert_eq!(child.process.wait().unwrap(), exit_status);
}

#[test]
fn stop_owned_child_without_metadata_is_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = spawn_test_child(directory.path());

    stop_owned_child(directory.path(), &mut child.process).unwrap();

    let exit_status = child
        .process
        .try_wait()
        .unwrap()
        .expect("an owned child must be stopped even without daemon metadata");
    assert!(!exit_status.success());
    assert_eq!(child.process.wait().unwrap(), exit_status);
    stop_owned_child(directory.path(), &mut child.process).unwrap();
    assert_eq!(child.process.wait().unwrap(), exit_status);
    assert!(read_daemon_info(directory.path()).is_none());
}

#[test]
fn stop_owned_child_removes_its_metadata_when_already_reaped() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = spawn_test_child(directory.path());
    let info = DaemonInfo {
        pid: child.process.id(),
        ..sample_info()
    };
    write_daemon_info(directory.path(), &info).unwrap();
    child.process.kill().unwrap();
    let exit_status = child.process.wait().unwrap();

    stop_owned_child(directory.path(), &mut child.process).unwrap();

    assert_eq!(child.process.wait().unwrap(), exit_status);
    assert!(read_daemon_info(directory.path()).is_none());
}
