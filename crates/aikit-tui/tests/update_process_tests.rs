use aikit_tui::update::{prepare_installation, validate_candidate};

#[tokio::test]
async fn candidate_version_can_be_checked_from_a_path_with_spaces_and_quotes() {
    let directory = tempfile::tempdir().unwrap();
    let name = if cfg!(windows) {
        "new build's aikit.exe"
    } else {
        "new build's aikit"
    };
    let candidate = directory.path().join(name);
    std::fs::copy(env!("CARGO_BIN_EXE_aikit"), &candidate).unwrap();

    validate_candidate(&candidate, env!("CARGO_PKG_VERSION"))
        .await
        .unwrap();
}

#[tokio::test]
async fn candidate_with_the_wrong_version_is_rejected_before_installation() {
    let directory = tempfile::tempdir().unwrap();
    let candidate = directory
        .path()
        .join(aikit_core::updater::binary_file_name());
    std::fs::copy(env!("CARGO_BIN_EXE_aikit"), &candidate).unwrap();

    let error = validate_candidate(&candidate, "999.999.999")
        .await
        .unwrap_err();

    assert!(error.to_string().contains("version mismatch"));
    assert!(candidate.is_file());
}

#[tokio::test]
async fn prepared_candidate_can_be_executed_before_commit_and_terminal_failure_rolls_back() {
    let directory = tempfile::tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    let target = directory
        .path()
        .join(aikit_core::updater::binary_file_name());
    let candidate = aikit_core::updater::pending_update_path(directory.path());
    std::fs::create_dir_all(candidate.parent().unwrap()).unwrap();
    std::fs::write(&target, b"old executable").unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_aikit"), &candidate).unwrap();

    let prepared = prepare_installation(&config_path, env!("CARGO_PKG_VERSION"), &target)
        .await
        .unwrap();
    assert_eq!(
        std::fs::metadata(&target).unwrap().len(),
        std::fs::metadata(&candidate).unwrap().len()
    );
    let result = prepared
        .launch(Vec::new(), || {
            Err::<(), _>(aikit_core::AikitError::Provider(
                "no interactive terminal".into(),
            ))
        })
        .await;

    assert!(result.is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"old executable");
    assert!(candidate.is_file());
}

#[tokio::test]
async fn failed_candidate_validation_leaves_the_installed_binary_and_download_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    let target = directory
        .path()
        .join(aikit_core::updater::binary_file_name());
    let candidate = aikit_core::updater::pending_update_path(directory.path());
    std::fs::create_dir_all(candidate.parent().unwrap()).unwrap();
    std::fs::write(&target, b"old executable").unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_aikit"), &candidate).unwrap();

    let result = prepare_installation(&config_path, "999.0.0", &target).await;

    assert!(result.is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"old executable");
    assert!(candidate.is_file());
}
