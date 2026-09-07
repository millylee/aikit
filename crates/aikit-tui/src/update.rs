use std::{
    cell::RefCell,
    ffi::OsString,
    future::Future,
    path::{Path, PathBuf},
    process::Child,
};

use aikit_core::{
    config::{aikit_dir_for_config, load_state, save_state},
    updater::{self, BinaryInstallation, PreparedBinaryInstallation, StageUpdateOutcome},
    AikitError, Result,
};
use aikit_daemon::lifecycle::{self, DaemonInfo, StatusReport};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::AppState;

mod handoff;
pub use handoff::{validate_candidate, StartupHandoff};

pub fn pending_version_for_startup(state: &AppState) -> Result<Option<String>> {
    let aikit_dir = aikit_dir_for_config(&state.config_path);
    if !updater::pending_update_path(&aikit_dir).is_file() {
        return Ok(None);
    }
    let version = updater::pending_update_version(&aikit_dir)?
        .or_else(|| state.config.update_prompt.pending_version.clone());
    Ok(version.filter(|version| updater::version_is_newer(version, env!("CARGO_PKG_VERSION"))))
}

#[derive(Debug)]
pub enum UpdateEvent {
    Downloading(String),
    Finished(Result<StageUpdateOutcome>),
}

pub fn spawn_update_check(
    client: reqwest::Client,
    latest_release_url: String,
    aikit_dir: PathBuf,
    skipped_version: Option<String>,
    sender: UnboundedSender<UpdateEvent>,
) -> tokio::task::JoinHandle<()> {
    let forwarder_sender = sender.clone();
    let worker = tokio::spawn(async move {
        let progress_sender = sender.clone();
        let result = updater::stage_update_with_progress(
            &client,
            &latest_release_url,
            &aikit_dir,
            skipped_version.as_deref(),
            move |version| {
                let _ = progress_sender.send(UpdateEvent::Downloading(version.to_string()));
            },
        )
        .await;
        let _ = sender.send(UpdateEvent::Finished(result));
    });
    tokio::spawn(forward_worker_result(worker, forwarder_sender))
}

/// A crashed worker must still deliver `Finished` so `update_in_progress` is
/// always reset instead of silently blocking further `u` presses.
async fn forward_worker_result(
    worker: tokio::task::JoinHandle<()>,
    sender: UnboundedSender<UpdateEvent>,
) {
    if let Err(join_error) = worker.await {
        let _ = sender.send(UpdateEvent::Finished(Err(AikitError::Provider(format!(
            "update check worker crashed: {join_error}"
        )))));
    }
}

trait UpdateDaemon {
    async fn running_info(&self) -> Result<Option<DaemonInfo>>;
    async fn stop(&self) -> Result<()>;
    async fn start(&self, executable: &Path, info: &DaemonInfo) -> Result<()>;
}

struct ManagedDaemon {
    aikit_dir: PathBuf,
    child: RefCell<Option<Child>>,
}

impl UpdateDaemon for ManagedDaemon {
    async fn running_info(&self) -> Result<Option<DaemonInfo>> {
        match lifecycle::status(&self.aikit_dir).await? {
            StatusReport::Running { info } => Ok(Some(info)),
            StatusReport::Stopped | StatusReport::Stale { .. } => Ok(None),
        }
    }

    async fn stop(&self) -> Result<()> {
        let owned_child = self.child.borrow_mut().take();
        if let Some(mut child) = owned_child {
            if let Err(err) = lifecycle::stop_owned_child(&self.aikit_dir, &mut child) {
                *self.child.borrow_mut() = Some(child);
                return Err(err);
            }
            return Ok(());
        }
        lifecycle::stop(&self.aikit_dir).await?;
        Ok(())
    }

    async fn start(&self, executable: &Path, info: &DaemonInfo) -> Result<()> {
        let (_, child) = lifecycle::start_with_executable_owned(
            &self.aikit_dir,
            info.bind,
            info.port,
            executable,
        )
        .await?;
        if let Some(child) = child {
            *self.child.borrow_mut() = Some(child);
        }
        Ok(())
    }
}

pub struct PreparedUpdate {
    installation: BinaryInstallation,
    daemon_info: Option<DaemonInfo>,
    config_path: PathBuf,
    version: String,
    started_daemon: Option<Child>,
}

pub async fn prepare_installation(
    config_path: &Path,
    version: &str,
    executable: &Path,
) -> Result<PreparedUpdate> {
    let aikit_dir = aikit_dir_for_config(config_path);
    let staged = updater::pending_update_path(&aikit_dir);
    let prepared = updater::prepare_binary_installation(&staged, executable)?;
    validate_candidate(prepared.candidate_path(), version).await?;
    let daemon = ManagedDaemon {
        aikit_dir,
        child: RefCell::new(None),
    };
    let (installation, daemon_info) = install_with_daemon(prepared, &daemon).await?;
    Ok(PreparedUpdate {
        installation,
        daemon_info,
        config_path: config_path.to_path_buf(),
        version: version.to_string(),
        started_daemon: daemon.child.into_inner(),
    })
}

impl PreparedUpdate {
    pub async fn launch<Guard>(
        mut self,
        arguments: Vec<OsString>,
        prepare_terminal: impl FnOnce() -> Result<Guard>,
    ) -> Result<(Child, Guard)> {
        let daemon = ManagedDaemon {
            aikit_dir: aikit_dir_for_config(&self.config_path),
            child: RefCell::new(self.started_daemon.take()),
        };
        self.launch_with(&daemon, move |target, aikit_dir, version| async move {
            let guard = prepare_terminal()?;
            let child =
                handoff::spawn_ready_child(&target, &arguments, &aikit_dir, &version).await?;
            Ok((child, guard))
        })
        .await
    }

    async fn launch_with<Output, Launch, LaunchFuture>(
        self,
        daemon: &impl UpdateDaemon,
        launch: Launch,
    ) -> Result<Output>
    where
        Launch: FnOnce(PathBuf, PathBuf, String) -> LaunchFuture,
        LaunchFuture: Future<Output = Result<Output>>,
    {
        let target = self.installation.target_path().to_path_buf();
        let result = launch(
            target,
            aikit_dir_for_config(&self.config_path),
            self.version.clone(),
        )
        .await;
        match result {
            Ok(child) => Ok(child),
            Err(err) => {
                let recovered =
                    rollback_installation(self.installation, &self.daemon_info, daemon).await;
                let err = recovery_error(err, recovered);
                let restored_metadata = (|| {
                    let mut state = load_state(&self.config_path)?;
                    state.update_prompt.pending_version = Some(self.version);
                    save_state(&self.config_path, &state)
                })();
                Err(recovery_error(err, restored_metadata))
            }
        }
    }
}

async fn install_with_daemon(
    prepared: PreparedBinaryInstallation,
    daemon: &impl UpdateDaemon,
) -> Result<(BinaryInstallation, Option<DaemonInfo>)> {
    let target = prepared.target_path().to_path_buf();
    let info = daemon.running_info().await?;
    if let Some(info) = &info {
        if let Err(err) = daemon.stop().await {
            return Err(recovery_error(err, daemon.start(&target, info).await));
        }
    }
    let installation = match prepared.commit() {
        Ok(installation) => installation,
        Err(err) => {
            let recovery = match &info {
                Some(info) => daemon.start(&target, info).await,
                None => Ok(()),
            };
            return Err(recovery_error(err, recovery));
        }
    };
    if let Some(info) = &info {
        if let Err(err) = daemon.start(&target, info).await {
            let recovery = rollback_installation(installation, &Some(info.clone()), daemon).await;
            return Err(recovery_error(err, recovery));
        }
    }
    Ok((installation, info))
}

async fn rollback_installation(
    installation: BinaryInstallation,
    info: &Option<DaemonInfo>,
    daemon: &impl UpdateDaemon,
) -> Result<()> {
    if info.is_some() {
        daemon.stop().await?;
    }
    let target = installation.target_path().to_path_buf();
    installation.rollback()?;
    if let Some(info) = info {
        daemon.start(&target, info).await?;
    }
    Ok(())
}

fn recovery_error(error: AikitError, recovery: Result<()>) -> AikitError {
    match recovery {
        Ok(()) => error,
        Err(recovery_error) => AikitError::Provider(format!(
            "{error}; recovery failed: {recovery_error}; update files have been retained"
        )),
    }
}

#[cfg(test)]
mod tests;
