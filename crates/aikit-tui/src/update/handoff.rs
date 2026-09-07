use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

use aikit_core::{AikitError, Result};

use crate::app::AppState;

const HANDOFF_ENV: &str = "AIKIT_UPDATE_HANDOFF";
const HANDOFF_TIMEOUT: Duration = Duration::from_secs(15);

pub struct StartupHandoff {
    path: PathBuf,
    version: String,
}

impl StartupHandoff {
    pub fn from_environment(aikit_dir: &Path, version: &str) -> Result<Option<Self>> {
        let Some(path) = std::env::var_os(HANDOFF_ENV) else {
            return Ok(None);
        };
        Self::from_path(Path::new(&path), aikit_dir, version)
    }

    fn from_path(path: &Path, aikit_dir: &Path, version: &str) -> Result<Option<Self>> {
        let path = path.canonicalize()?;
        let expected_parent = aikit_dir.canonicalize()?;
        let valid_name = path.file_name().is_some_and(|name| {
            let name = name.to_string_lossy();
            name.starts_with("aikit-handoff-") && name.ends_with(".ready")
        });
        if !valid_name || path.parent() != Some(expected_parent.as_path()) {
            return Err(AikitError::Provider("invalid update handoff path".into()));
        }
        if fs::read_to_string(&path)? != format!("pending {version}") {
            return Err(AikitError::Provider(
                "update handoff version mismatch".into(),
            ));
        }
        Ok(Some(Self {
            path,
            version: version.to_string(),
        }))
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn acknowledge(&self, state: &mut AppState) -> Result<()> {
        state.clear_pending_update_version()?;
        let mut marker = tempfile::NamedTempFile::new_in(self.path.parent().unwrap())?;
        write!(marker, "ready {}", self.version)?;
        marker.as_file().sync_all()?;
        marker
            .persist(&self.path)
            .map_err(|err| AikitError::Io(err.error))?;
        Ok(())
    }
}

struct HandoffTicket {
    path: tempfile::TempPath,
    version: String,
}

impl HandoffTicket {
    fn new(aikit_dir: &Path, version: &str) -> Result<Self> {
        fs::create_dir_all(aikit_dir)?;
        let mut file = tempfile::Builder::new()
            .prefix("aikit-handoff-")
            .suffix(".ready")
            .tempfile_in(aikit_dir)?;
        write!(file, "pending {version}")?;
        file.as_file().sync_all()?;
        Ok(Self {
            path: file.into_temp_path(),
            version: version.to_string(),
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

pub(super) async fn spawn_ready_child(
    executable: &Path,
    arguments: &[OsString],
    aikit_dir: &Path,
    version: &str,
) -> Result<Child> {
    let ticket = HandoffTicket::new(aikit_dir, version)?;
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    spawn_and_wait_for_ack(command, &ticket, HANDOFF_TIMEOUT).await
}

async fn spawn_and_wait_for_ack(
    mut command: Command,
    ticket: &HandoffTicket,
    timeout: Duration,
) -> Result<Child> {
    let mut child = command.env(HANDOFF_ENV, ticket.path()).spawn()?;
    wait_for_ack(&mut child, ticket, timeout).await?;
    Ok(child)
}

async fn wait_for_ack(child: &mut Child, ticket: &HandoffTicket, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let result = async {
        loop {
            match fs::read_to_string(ticket.path()) {
                Ok(content) if content == format!("ready {}", ticket.version) => return Ok(()),
                // A pending or momentarily unreadable ticket (e.g. antivirus briefly
                // holding the file) keeps polling until the deadline.
                Ok(_) => {}
                Err(err)
                    if err.kind() == std::io::ErrorKind::NotFound
                        || err.kind() == std::io::ErrorKind::PermissionDenied => {}
                Err(err) => return Err(AikitError::Io(err)),
            }
            if let Some(status) = child.try_wait()? {
                return Err(AikitError::Provider(format!(
                    "updated TUI exited before initialization (code {status})"
                )));
            }
            if Instant::now() >= deadline {
                return Err(AikitError::Provider(
                    "updated TUI initialization timed out".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    .await;
    result.map_err(|error| child_failure(error, child))
}

pub async fn validate_candidate(executable: &Path, expected_version: &str) -> Result<()> {
    let mut command = Command::new(executable.canonicalize()?);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn()?;
    wait_for_exit(&mut child, Duration::from_secs(10))
        .await
        .map_err(|error| child_failure(error, &mut child))?;
    let output = child.wait_with_output()?;
    let actual_version = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || actual_version.trim() != format!("aikit {expected_version}") {
        return Err(AikitError::Provider(format!(
            "update candidate version mismatch: expected aikit {expected_version}, got {} ({})",
            actual_version.trim().chars().take(160).collect::<String>(),
            output.status
        )));
    }
    Ok(())
}

async fn wait_for_exit(child: &mut Child, timeout: Duration) -> Result<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(AikitError::Provider(
                "update candidate validation timed out".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn child_failure(error: AikitError, child: &mut Child) -> AikitError {
    let cleanup = (|| {
        if child.try_wait().ok().flatten().is_none() {
            if let Err(kill_error) = child.kill() {
                if child.try_wait().ok().flatten().is_none() {
                    return Err(kill_error);
                }
            }
        }
        child.wait()?;
        Ok::<(), std::io::Error>(())
    })();
    match cleanup {
        Ok(()) => error,
        Err(cleanup_error) => AikitError::Provider(format!(
            "{error}; failed to terminate update child: {cleanup_error}"
        )),
    }
}

#[cfg(test)]
mod tests;
