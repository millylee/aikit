use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

use aikit_core::updater::{
    cleanup_previous_binary, install_binary, install_binary_with_backup,
    prepare_binary_installation,
};

fn prepared_candidate_paths(directory: &Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".aikit-update-")
        })
        .collect()
}

#[test]
fn preparation_copies_candidate_without_changing_target_or_backup() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    let backup = directory.path().join("aikit.exe.aikit-backup");
    fs::write(&target, b"previous binary").unwrap();
    fs::write(&staged, b"updated binary").unwrap();
    fs::write(&backup, b"older backup").unwrap();

    let prepared = prepare_binary_installation(&staged, &target).unwrap();

    assert_eq!(prepared.target_path(), target);
    assert_ne!(prepared.candidate_path(), staged);
    assert_eq!(prepared.candidate_path().parent(), target.parent());
    assert_eq!(
        fs::read(prepared.candidate_path()).unwrap(),
        b"updated binary"
    );
    assert_eq!(fs::read(&target).unwrap(), b"previous binary");
    assert_eq!(fs::read(&backup).unwrap(), b"older backup");
    assert_eq!(fs::read(&staged).unwrap(), b"updated binary");
    assert_eq!(prepared_candidate_paths(directory.path()).len(), 1);
    fs::write(&staged, b"changed after preparation").unwrap();
    assert_eq!(
        fs::read(prepared.candidate_path()).unwrap(),
        b"updated binary"
    );

    let installation = prepared.commit().unwrap();

    assert_eq!(installation.target_path(), target);
    assert_eq!(installation.backup_path(), Some(backup.as_path()));
    assert_eq!(fs::read(&target).unwrap(), b"updated binary");
    assert_eq!(fs::read(&backup).unwrap(), b"previous binary");
    assert_eq!(fs::read(&staged).unwrap(), b"changed after preparation");
    assert!(prepared_candidate_paths(directory.path()).is_empty());
    installation.rollback().unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"previous binary");
}

#[test]
fn prepared_candidate_can_be_executed_before_commit() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = std::env::current_exe().unwrap();
    fs::write(&target, b"previous binary").unwrap();
    let prepared = prepare_binary_installation(&staged, &target).unwrap();
    let candidate = prepared.candidate_path().to_path_buf();

    let output = Command::new(prepared.candidate_path())
        .arg("--list")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout)
        .contains("prepared_candidate_can_be_executed_before_commit: test"));
    assert_eq!(fs::read(&target).unwrap(), b"previous binary");
    drop(prepared);
    assert!(!candidate.exists());
    assert!(prepare_binary_installation(&staged, &target).is_ok());
}

#[test]
fn dropping_preparation_preserves_target_and_backup_and_releases_lock() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    let backup = directory.path().join("aikit.exe.aikit-backup");
    fs::write(&target, b"previous binary").unwrap();
    fs::write(&staged, b"updated binary").unwrap();
    fs::write(&backup, b"older backup").unwrap();
    let prepared = prepare_binary_installation(&staged, &target).unwrap();

    drop(prepared);

    assert_eq!(fs::read(&target).unwrap(), b"previous binary");
    assert_eq!(fs::read(&backup).unwrap(), b"older backup");
    assert_eq!(fs::read(&staged).unwrap(), b"updated binary");
    assert!(prepared_candidate_paths(directory.path()).is_empty());
    let installation = prepare_binary_installation(&staged, &target)
        .unwrap()
        .commit()
        .unwrap();
    drop(installation);
    cleanup_previous_binary(&target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"updated binary");
    assert!(!backup.exists());
}

#[test]
fn preparation_holds_the_same_lock_through_commit_and_handoff() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    let backup = directory.path().join("aikit.exe.aikit-backup");
    fs::write(&target, b"previous binary").unwrap();
    fs::write(&staged, b"updated binary").unwrap();
    fs::write(&backup, b"older backup").unwrap();
    let prepared = prepare_binary_installation(&staged, &target).unwrap();

    let error = prepare_binary_installation(&staged, &target).unwrap_err();
    assert!(error
        .to_string()
        .contains("cannot lock executable for update"));
    assert!(install_binary_with_backup(&staged, &target).is_err());
    assert!(cleanup_previous_binary(&target).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"previous binary");
    assert_eq!(fs::read(&backup).unwrap(), b"older backup");
    assert_eq!(prepared_candidate_paths(directory.path()).len(), 1);

    let installation = prepared.commit().unwrap();

    assert!(prepare_binary_installation(&staged, &target).is_err());
    assert!(install_binary_with_backup(&staged, &target).is_err());
    assert!(cleanup_previous_binary(&target).is_err());
    assert_eq!(fs::read(&backup).unwrap(), b"previous binary");
    installation.rollback().unwrap();
    assert!(prepare_binary_installation(&staged, &target).is_ok());
    assert!(prepared_candidate_paths(directory.path()).is_empty());
}

#[test]
fn preparation_leaves_a_missing_target_absent_until_commit() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("bin/aikit.exe");
    let staged = directory.path().join("pending.exe");
    fs::write(&staged, b"updated binary").unwrap();

    let prepared = prepare_binary_installation(&staged, &target).unwrap();

    assert!(!target.exists());
    assert!(cleanup_previous_binary(&target).is_err());
    let installation = prepared.commit().unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"updated binary");
    assert!(installation.backup_path().is_none());
    installation.rollback().unwrap();
    assert!(!target.exists());
    assert_eq!(fs::read(&staged).unwrap(), b"updated binary");
    assert!(prepared_candidate_paths(target.parent().unwrap()).is_empty());
    assert!(prepare_binary_installation(&staged, &target).is_ok());
}

#[test]
fn commit_rejects_a_target_that_became_a_directory() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    fs::write(&target, b"previous binary").unwrap();
    fs::write(&staged, b"updated binary").unwrap();
    let prepared = prepare_binary_installation(&staged, &target).unwrap();
    fs::remove_file(&target).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep.txt"), b"keep").unwrap();

    assert!(prepared.commit().is_err());

    assert_eq!(fs::read(target.join("keep.txt")).unwrap(), b"keep");
    assert_eq!(fs::read(&staged).unwrap(), b"updated binary");
    assert!(!directory.path().join("aikit.exe.aikit-backup").exists());
    assert!(prepared_candidate_paths(directory.path()).is_empty());
    cleanup_previous_binary(&target).unwrap();
}

#[test]
fn commit_failure_before_replacement_preserves_target_and_releases_lock() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    let backup = directory.path().join("aikit.exe.aikit-backup");
    fs::write(&target, b"previous binary").unwrap();
    fs::write(&staged, b"updated binary").unwrap();
    let prepared = prepare_binary_installation(&staged, &target).unwrap();
    fs::create_dir(&backup).unwrap();
    fs::write(backup.join("keep.txt"), b"keep").unwrap();

    assert!(prepared.commit().is_err());

    assert_eq!(fs::read(&target).unwrap(), b"previous binary");
    assert_eq!(fs::read(&staged).unwrap(), b"updated binary");
    assert_eq!(fs::read(backup.join("keep.txt")).unwrap(), b"keep");
    assert!(prepared_candidate_paths(directory.path()).is_empty());
    assert!(prepare_binary_installation(&staged, &target).is_ok());
}

#[test]
fn commit_restores_target_when_the_prepared_copy_cannot_be_published() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    fs::write(&target, b"previous binary").unwrap();
    fs::write(&staged, b"updated binary").unwrap();
    let prepared = prepare_binary_installation(&staged, &target).unwrap();
    fs::remove_file(prepared.candidate_path()).unwrap();

    assert!(prepared.commit().is_err());

    assert_eq!(fs::read(&target).unwrap(), b"previous binary");
    assert_eq!(fs::read(&staged).unwrap(), b"updated binary");
    assert!(!directory.path().join("aikit.exe.aikit-backup").exists());
    assert!(prepared_candidate_paths(directory.path()).is_empty());
    assert!(prepare_binary_installation(&staged, &target).is_ok());
}

#[test]
fn cleanup_of_a_missing_parent_does_not_create_directories() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("missing/aikit.exe");

    cleanup_previous_binary(&target).unwrap();

    assert!(!target.parent().unwrap().exists());
}

#[test]
fn replacement_does_not_modify_an_open_previous_binary() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    fs::write(&target, b"previous binary").unwrap();
    fs::write(&staged, b"updated binary").unwrap();
    let mut previous = fs::File::open(&target).unwrap();

    install_binary(&staged, &target).unwrap();

    let mut previous_contents = Vec::new();
    previous.read_to_end(&mut previous_contents).unwrap();
    assert_eq!(previous_contents, b"previous binary");
    assert_eq!(fs::read(&target).unwrap(), b"updated binary");
    assert_eq!(fs::read(&staged).unwrap(), b"updated binary");
}

#[cfg(windows)]
#[test]
fn replacement_succeeds_when_the_previous_binary_denies_writes() {
    use std::os::windows::fs::OpenOptionsExt;

    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    fs::write(&target, b"previous binary").unwrap();
    fs::write(&staged, b"updated binary").unwrap();
    let mut previous = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x0000_0001 | 0x0000_0004)
        .open(&target)
        .unwrap();

    install_binary(&staged, &target).unwrap();

    let mut previous_contents = Vec::new();
    previous.read_to_end(&mut previous_contents).unwrap();
    assert_eq!(previous_contents, b"previous binary");
    assert_eq!(fs::read(&target).unwrap(), b"updated binary");
}

#[test]
fn missing_candidate_does_not_change_the_installed_binary() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    fs::write(&target, b"previous binary").unwrap();

    assert!(install_binary(&directory.path().join("missing.exe"), &target).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"previous binary");
}

#[test]
fn failed_replacement_preserves_the_candidate_and_existing_directory() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep.txt"), b"keep").unwrap();
    fs::write(&staged, b"updated binary").unwrap();

    assert!(install_binary(&staged, &target).is_err());
    assert_eq!(fs::read(target.join("keep.txt")).unwrap(), b"keep");
    assert_eq!(fs::read(&staged).unwrap(), b"updated binary");
}

#[test]
fn replacement_can_create_a_missing_target() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("bin").join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    fs::write(&staged, b"updated binary").unwrap();

    install_binary(&staged, &target).unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"updated binary");
    assert_eq!(fs::read(&staged).unwrap(), b"updated binary");
}

#[test]
fn failed_update_can_restore_the_previous_binary_without_losing_the_candidate() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    fs::write(&target, b"previous binary").unwrap();
    fs::write(&staged, b"updated binary").unwrap();

    let installation = install_binary_with_backup(&staged, &target).unwrap();
    assert_eq!(
        fs::read(installation.backup_path().unwrap()).unwrap(),
        b"previous binary"
    );
    installation.rollback().unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"previous binary");
    assert_eq!(fs::read(&staged).unwrap(), b"updated binary");
}

#[test]
fn another_installation_cannot_overwrite_a_pending_handoff() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    fs::write(&target, b"previous binary").unwrap();
    fs::write(&staged, b"updated binary").unwrap();
    let installation = install_binary_with_backup(&staged, &target).unwrap();

    assert!(install_binary_with_backup(&staged, &target).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"updated binary");
    installation.rollback().unwrap();
    assert!(install_binary_with_backup(&staged, &target).is_ok());
}

#[test]
fn cleanup_cannot_remove_a_backup_during_pending_handoff() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    fs::write(&target, b"previous binary").unwrap();
    fs::write(&staged, b"updated binary").unwrap();
    let installation = install_binary_with_backup(&staged, &target).unwrap();
    let backup = installation.backup_path().unwrap().to_path_buf();

    assert!(cleanup_previous_binary(&target).is_err());
    assert_eq!(fs::read(&backup).unwrap(), b"previous binary");
    assert_eq!(fs::read(&target).unwrap(), b"updated binary");
    installation.rollback().unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"previous binary");
    cleanup_previous_binary(&target).unwrap();
}

#[test]
fn the_installed_binary_cannot_be_its_own_candidate() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    fs::write(&target, b"previous binary").unwrap();

    assert!(install_binary(&target, &target).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"previous binary");
}

#[test]
fn an_unused_backup_can_be_cleaned_without_touching_the_update() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("aikit.exe");
    let staged = directory.path().join("pending.exe");
    fs::write(&target, b"previous binary").unwrap();
    fs::write(&staged, b"updated binary").unwrap();
    let installation = install_binary_with_backup(&staged, &target).unwrap();
    let backup = installation.backup_path().unwrap().to_path_buf();
    drop(installation);

    cleanup_previous_binary(&target).unwrap();

    assert!(!backup.exists());
    assert_eq!(fs::read(&target).unwrap(), b"updated binary");
    assert_eq!(fs::read(&staged).unwrap(), b"updated binary");
}
