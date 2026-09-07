use super::*;

#[test]
fn handoff_child() {
    let Some(mode) = std::env::var_os("AIKIT_HANDOFF_TEST_CHILD") else {
        return;
    };
    if mode == "ready" {
        let path = PathBuf::from(std::env::var_os(HANDOFF_ENV).unwrap());
        let pending = fs::read_to_string(&path).unwrap();
        let directory = path.parent().unwrap();
        let version = pending.strip_prefix("pending ").unwrap();
        let receipt = StartupHandoff::from_path(&path, directory, version)
            .unwrap()
            .unwrap();
        let mut state = crate::app::AppState::new(directory.join("config.toml"));
        state.config.update_prompt.pending_version = Some(version.into());
        receipt.acknowledge(&mut state).unwrap();
        std::thread::sleep(Duration::from_millis(500));
    } else if mode == "timeout" {
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn child_command(mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "update::handoff::tests::handoff_child"])
        .env("AIKIT_HANDOFF_TEST_CHILD", mode)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

#[tokio::test]
async fn parent_waits_for_a_child_ready_acknowledgment() {
    let directory = tempfile::tempdir().unwrap();
    let ticket = HandoffTicket::new(directory.path(), "999.0.0").unwrap();

    let mut child = spawn_and_wait_for_ack(child_command("ready"), &ticket, Duration::from_secs(5))
        .await
        .unwrap();

    assert_eq!(fs::read_to_string(ticket.path()).unwrap(), "ready 999.0.0");
    assert!(child.wait().unwrap().success());
}

#[tokio::test]
async fn successful_exit_without_acknowledgment_is_not_a_successful_handoff() {
    let directory = tempfile::tempdir().unwrap();
    let ticket = HandoffTicket::new(directory.path(), "999.0.0").unwrap();

    let result =
        spawn_and_wait_for_ack(child_command("exit"), &ticket, Duration::from_secs(5)).await;

    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(ticket.path()).unwrap(),
        "pending 999.0.0"
    );
}

#[tokio::test]
async fn a_handoff_timeout_terminates_its_owned_child() {
    let directory = tempfile::tempdir().unwrap();
    let ticket = HandoffTicket::new(directory.path(), "999.0.0").unwrap();
    let mut command = child_command("timeout");
    command.env(HANDOFF_ENV, ticket.path());
    let mut child = command.spawn().unwrap();

    assert!(
        wait_for_ack(&mut child, &ticket, Duration::from_millis(100))
            .await
            .is_err()
    );
    assert!(child.try_wait().unwrap().is_some());
}

#[tokio::test]
async fn a_transiently_unreadable_ticket_waits_for_the_deadline_instead_of_failing() {
    let directory = tempfile::tempdir().unwrap();
    let ticket = HandoffTicket::new(directory.path(), "999.0.0").unwrap();
    fs::remove_file(ticket.path()).unwrap();
    let mut command = child_command("timeout");
    command.env(HANDOFF_ENV, ticket.path());
    let mut child = command.spawn().unwrap();

    let error = wait_for_ack(&mut child, &ticket, Duration::from_millis(150))
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("timed out"), "unexpected error: {error}");
}

#[test]
fn child_only_accepts_a_ticket_for_its_own_version_and_directory() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let ticket = HandoffTicket::new(directory.path(), "999.0.0").unwrap();

    assert!(StartupHandoff::from_path(ticket.path(), directory.path(), "998.0.0").is_err());
    assert!(StartupHandoff::from_path(ticket.path(), outside.path(), "999.0.0").is_err());
    let receipt = StartupHandoff::from_path(ticket.path(), directory.path(), "999.0.0")
        .unwrap()
        .unwrap();
    let mut state = crate::app::AppState::new(directory.path().join("config.toml"));
    state.config.update_prompt.pending_version = Some("999.0.0".into());
    receipt.acknowledge(&mut state).unwrap();
    assert_eq!(fs::read_to_string(ticket.path()).unwrap(), "ready 999.0.0");
    assert_eq!(
        aikit_core::config::load_state(&state.config_path)
            .unwrap()
            .update_prompt
            .pending_version,
        None
    );
}

#[test]
fn a_missing_handoff_ticket_is_an_error_not_a_new_installation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("aikit-handoff-missing.ready");

    assert!(StartupHandoff::from_path(&path, directory.path(), "999.0.0").is_err());
}

#[test]
fn failed_metadata_persistence_never_acknowledges_or_deletes_the_candidate() {
    let directory = tempfile::tempdir().unwrap();
    let ticket = HandoffTicket::new(directory.path(), "999.0.0").unwrap();
    let receipt = StartupHandoff::from_path(ticket.path(), directory.path(), "999.0.0")
        .unwrap()
        .unwrap();
    let mut state = crate::app::AppState::new(directory.path().join("config.toml"));
    state.config.update_prompt.pending_version = Some("999.0.0".into());
    fs::create_dir(aikit_core::config::state_path(&state.config_path)).unwrap();
    let pending = aikit_core::updater::pending_update_path(directory.path());
    fs::create_dir_all(pending.parent().unwrap()).unwrap();
    fs::write(&pending, "candidate").unwrap();

    assert!(receipt.acknowledge(&mut state).is_err());

    assert_eq!(
        fs::read_to_string(ticket.path()).unwrap(),
        "pending 999.0.0"
    );
    assert_eq!(
        state.config.update_prompt.pending_version,
        Some("999.0.0".into())
    );
    assert_eq!(fs::read_to_string(pending).unwrap(), "candidate");
}
