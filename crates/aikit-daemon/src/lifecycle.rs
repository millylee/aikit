use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use aikit_core::{AikitError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonInfo {
    pub pid: u32,
    pub port: u16,
}

pub fn daemon_info_path(aikit_dir: &Path) -> PathBuf {
    aikit_dir.join("daemon.json")
}

pub fn read_daemon_info(aikit_dir: &Path) -> Option<DaemonInfo> {
    let data = fs::read_to_string(daemon_info_path(aikit_dir)).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn write_daemon_info(aikit_dir: &Path, info: &DaemonInfo) -> Result<()> {
    let path = daemon_info_path(aikit_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(info)
        .map_err(|err| AikitError::Provider(format!("failed to serialize daemon info: {err}")))?;
    fs::write(&path, data)?;
    set_owner_only(&path)?;
    Ok(())
}

pub fn remove_daemon_info(aikit_dir: &Path) {
    let _ = fs::remove_file(daemon_info_path(aikit_dir));
}

pub async fn probe_alive(port: u16) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };
    let url = format!("http://127.0.0.1:{port}/api/health");
    matches!(
        client.get(&url).send().await, Ok(response) if response.status().is_success()
    )
}

pub async fn status(aikit_dir: &Path) -> Result<StatusReport> {
    let Some(info) = read_daemon_info(aikit_dir) else {
        return Ok(StatusReport::Stopped);
    };
    if probe_alive(info.port).await {
        Ok(StatusReport::Running { info })
    } else {
        Ok(StatusReport::Stale { info })
    }
}

pub async fn stop(aikit_dir: &Path) -> Result<StopOutcome> {
    let Some(info) = read_daemon_info(aikit_dir) else {
        return Ok(StopOutcome::NotRunning);
    };
    if !probe_alive(info.port).await {
        remove_daemon_info(aikit_dir);
        return Ok(StopOutcome::StaleRemoved);
    }
    terminate_pid(info.pid)?;
    for _ in 0..20 {
        if !probe_alive(info.port).await {
            remove_daemon_info(aikit_dir);
            return Ok(StopOutcome::Stopped { pid: info.pid });
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(AikitError::Provider(format!(
        "daemon pid {} did not stop within timeout",
        info.pid
    )))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusReport {
    Running { info: DaemonInfo },
    Stale { info: DaemonInfo },
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopOutcome {
    Stopped { pid: u32 },
    NotRunning,
    StaleRemoved,
}

fn terminate_pid(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .arg(pid.to_string())
            .status()
            .map_err(|err| AikitError::Provider(format!("failed to signal daemon: {err}")))?;
        if !status.success() {
            return Err(AikitError::Provider(format!(
                "failed to stop daemon pid {pid}"
            )));
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Stdio;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|err| AikitError::Provider(format!("failed to signal daemon: {err}")))?;
        if !status.success() {
            return Err(AikitError::Provider(format!(
                "failed to stop daemon pid {pid}"
            )));
        }
    }

    Ok(())
}

#[cfg(unix)]
fn set_owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> Result<()> {
    Ok(())
}
