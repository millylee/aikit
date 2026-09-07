use std::{cell::RefCell, fs};

use super::*;
use crate::app::AppState;

struct TestDaemon {
    info: Option<DaemonInfo>,
    events: RefCell<Vec<String>>,
    fail_stop: bool,
    fail_new_start: bool,
}

impl TestDaemon {
    fn running() -> Self {
        Self {
            info: Some(DaemonInfo {
                pid: 12345,
                bind: "127.0.0.1".parse().unwrap(),
                port: 17654,
            }),
            events: RefCell::new(Vec::new()),
            fail_stop: false,
            fail_new_start: false,
        }
    }
}

impl UpdateDaemon for TestDaemon {
    async fn running_info(&self) -> Result<Option<DaemonInfo>> {
        Ok(self.info.clone())
    }

    async fn stop(&self) -> Result<()> {
        self.events.borrow_mut().push("stop".into());
        if self.fail_stop {
            return Err(AikitError::Provider("stop failed".into()));
        }
        Ok(())
    }

    async fn start(&self, executable: &Path, info: &DaemonInfo) -> Result<()> {
        assert_eq!(Some(info), self.info.as_ref());
        let binary = fs::read_to_string(executable).unwrap();
        self.events.borrow_mut().push(format!("start:{binary}"));
        if self.fail_new_start && binary == "new" {
            return Err(AikitError::Provider("new daemon failed".into()));
        }
        Ok(())
    }
}

fn binary_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    fs::write(&target, "old").unwrap();
    fs::write(&staged, "new").unwrap();
    (directory, target, staged)
}

async fn install_with_daemon(
    staged: &Path,
    target: &Path,
    daemon: &impl UpdateDaemon,
) -> Result<(BinaryInstallation, Option<DaemonInfo>)> {
    let prepared = updater::prepare_binary_installation(staged, target)?;
    super::install_with_daemon(prepared, daemon).await
}

#[tokio::test]
async fn running_daemon_is_stopped_and_restored_with_the_new_binary() {
    let (_directory, target, staged) = binary_fixture();
    let daemon = TestDaemon::running();

    let (_installation, info) = install_with_daemon(&staged, &target, &daemon)
        .await
        .unwrap();

    assert_eq!(info, daemon.info);
    assert_eq!(*daemon.events.borrow(), ["stop", "start:new"]);
    assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    assert_eq!(fs::read_to_string(&staged).unwrap(), "new");
}

#[tokio::test]
async fn an_inactive_daemon_is_not_started_by_an_update() {
    let (_directory, target, staged) = binary_fixture();
    let mut daemon = TestDaemon::running();
    daemon.info = None;

    let (_installation, info) = install_with_daemon(&staged, &target, &daemon)
        .await
        .unwrap();

    assert!(info.is_none());
    assert!(daemon.events.borrow().is_empty());
    assert_eq!(fs::read_to_string(&target).unwrap(), "new");
}

#[tokio::test]
async fn stop_failure_keeps_the_old_binary_and_pending_download() {
    let (_directory, target, staged) = binary_fixture();
    let mut daemon = TestDaemon::running();
    daemon.fail_stop = true;

    assert!(install_with_daemon(&staged, &target, &daemon)
        .await
        .is_err());

    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(fs::read_to_string(&staged).unwrap(), "new");
    assert_eq!(*daemon.events.borrow(), ["stop", "start:old"]);
}

#[tokio::test]
async fn invalid_candidate_does_not_interrupt_the_old_daemon() {
    let (_directory, target, staged) = binary_fixture();
    let daemon = TestDaemon::running();

    assert!(
        install_with_daemon(&staged.with_extension("missing"), &target, &daemon)
            .await
            .is_err()
    );

    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(fs::read_to_string(&staged).unwrap(), "new");
    assert!(daemon.events.borrow().is_empty());
}

#[tokio::test]
async fn replacement_failure_restores_the_daemon_without_losing_either_binary() {
    let (_directory, target, staged) = binary_fixture();
    let daemon = TestDaemon::running();
    fs::create_dir(target.with_file_name("aikit.exe.aikit-backup")).unwrap();

    assert!(install_with_daemon(&staged, &target, &daemon)
        .await
        .is_err());

    assert_eq!(*daemon.events.borrow(), ["stop", "start:old"]);
    assert_eq!(fs::read_to_string(target).unwrap(), "old");
    assert_eq!(fs::read_to_string(staged).unwrap(), "new");
}

#[tokio::test]
async fn competing_installation_does_not_interrupt_the_old_daemon() {
    let (_directory, target, staged) = binary_fixture();
    let daemon = TestDaemon::running();
    let _installation = updater::install_binary_with_backup(&staged, &target).unwrap();

    assert!(install_with_daemon(&staged, &target, &daemon)
        .await
        .is_err());

    assert!(daemon.events.borrow().is_empty());
}

#[test]
fn startup_uses_the_candidate_version_instead_of_stale_config_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let mut state = AppState::new(directory.path().join("config.toml"));
    state.config.update_prompt.pending_version = Some("999.0.0".into());
    let pending = updater::pending_update_path(directory.path());
    fs::create_dir_all(pending.parent().unwrap()).unwrap();
    fs::write(pending, "candidate").unwrap();
    fs::write(directory.path().join("pending-update/version"), "998.0.0").unwrap();

    assert_eq!(
        pending_version_for_startup(&state).unwrap(),
        Some("998.0.0".into())
    );
}

#[test]
fn startup_accepts_legacy_pending_metadata_but_never_reinstalls_current_or_older_versions() {
    let directory = tempfile::tempdir().unwrap();
    let mut state = AppState::new(directory.path().join("config.toml"));
    state.config.update_prompt.pending_version = Some("999.0.0".into());
    assert_eq!(pending_version_for_startup(&state).unwrap(), None);
    let pending = updater::pending_update_path(directory.path());
    fs::create_dir_all(pending.parent().unwrap()).unwrap();
    fs::write(pending, "candidate").unwrap();
    assert_eq!(
        pending_version_for_startup(&state).unwrap(),
        Some("999.0.0".into())
    );

    for version in [env!("CARGO_PKG_VERSION"), "0.0.1"] {
        fs::write(directory.path().join("pending-update/version"), version).unwrap();
        assert_eq!(pending_version_for_startup(&state).unwrap(), None);
    }
}

#[tokio::test]
async fn a_crashed_update_worker_still_finishes_the_update_check() {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let panicking_worker = tokio::spawn(async {
        panic!("update worker exploded");
    });

    forward_worker_result(panicking_worker, sender).await;

    let event = tokio::time::timeout(std::time::Duration::from_secs(5), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, UpdateEvent::Finished(Err(_))));
}

#[tokio::test]
async fn worker_reports_download_before_a_delayed_download_failure() {
    use std::time::Duration;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    let directory = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/releases/latest"))
        .respond_with(ResponseTemplate::new(302).insert_header(
            "Location",
            format!("{}/releases/tag/v999.0.0", server.uri()),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/releases/tag/v999.0.0"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/releases/download/v999.0.0/{}",
            updater::release_archive_name().unwrap()
        )))
        .respond_with(ResponseTemplate::new(500).set_delay(Duration::from_millis(300)))
        .mount(&server)
        .await;
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();

    let worker = spawn_update_check(
        reqwest::Client::new(),
        format!("{}/releases/latest", server.uri()),
        directory.path().to_path_buf(),
        None,
        sender,
    );

    let event = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, UpdateEvent::Downloading(version) if version == "999.0.0"));
    assert!(!worker.is_finished());
    let event = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, UpdateEvent::Finished(Err(_))));
    worker.await.unwrap();
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn restart_failure_rolls_back_before_restoring_the_old_daemon() {
    let (_directory, target, staged) = binary_fixture();
    let mut daemon = TestDaemon::running();
    daemon.fail_new_start = true;

    assert!(install_with_daemon(&staged, &target, &daemon)
        .await
        .is_err());

    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(fs::read_to_string(&staged).unwrap(), "new");
    assert_eq!(
        *daemon.events.borrow(),
        ["stop", "start:new", "stop", "start:old"]
    );
}

#[tokio::test]
async fn failed_tui_handoff_restores_the_binary_service_and_pending_metadata() {
    let (directory, target, staged) = binary_fixture();
    let daemon = TestDaemon::running();
    let (installation, daemon_info) = install_with_daemon(&staged, &target, &daemon)
        .await
        .unwrap();
    let config_path = directory.path().join("config.toml");
    let update = PreparedUpdate {
        installation,
        daemon_info,
        config_path: config_path.clone(),
        version: "999.0.0".into(),
        started_daemon: None,
    };

    let result = update
        .launch_with(&daemon, |_, _, _| async {
            Err::<Child, _>(AikitError::Provider("new TUI exited before ready".into()))
        })
        .await;

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(fs::read_to_string(&staged).unwrap(), "new");
    assert_eq!(
        *daemon.events.borrow(),
        ["stop", "start:new", "stop", "start:old"]
    );
    assert_eq!(
        aikit_core::config::load_state(&config_path)
            .unwrap()
            .update_prompt
            .pending_version,
        Some("999.0.0".into())
    );
}

#[tokio::test]
async fn terminal_setup_failure_rolls_back_before_spawning_an_updated_tui() {
    let (directory, target, staged) = binary_fixture();
    let installation = updater::install_binary_with_backup(&staged, &target).unwrap();
    let update = PreparedUpdate {
        installation,
        daemon_info: None,
        config_path: directory.path().join("config.toml"),
        version: "999.0.0".into(),
        started_daemon: None,
    };

    let result = update
        .launch(Vec::new(), || {
            Err::<(), _>(AikitError::Provider("terminal setup failed".into()))
        })
        .await;

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "old");
    assert_eq!(fs::read_to_string(staged).unwrap(), "new");
}
