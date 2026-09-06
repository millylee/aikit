use std::net::IpAddr;

use aikit_daemon::lifecycle::{
    ensure_startable, format_http_url, read_daemon_info, remove_daemon_info, status, stop,
    write_daemon_info, DaemonInfo, StatusReport, StopOutcome,
};

fn sample_info() -> DaemonInfo {
    DaemonInfo {
        pid: 4242,
        port: 61234,
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
