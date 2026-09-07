use std::{
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use aikit_core::{AikitError, Result};
use serde::{Deserialize, Serialize};

const START_READINESS_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonInfo {
    pub pid: u32,
    pub port: u16,
    pub bind: IpAddr,
}

pub fn format_http_url(bind: IpAddr, port: u16) -> String {
    match bind {
        IpAddr::V4(addr) => format!("http://{addr}:{port}"),
        IpAddr::V6(addr) => format!("http://[{addr}]:{port}"),
    }
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

pub async fn probe_alive(bind: IpAddr, port: u16) -> bool {
    let client = match reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };
    let url = format!("{}/api/health", format_http_url(bind, port));
    matches!(
        client.get(&url).send().await, Ok(response) if response.status().is_success()
    )
}

pub async fn status(aikit_dir: &Path) -> Result<StatusReport> {
    let Some(info) = read_daemon_info(aikit_dir) else {
        return Ok(StatusReport::Stopped);
    };
    if probe_alive(info.bind, info.port).await {
        Ok(StatusReport::Running { info })
    } else {
        Ok(StatusReport::Stale { info })
    }
}

pub async fn stop(aikit_dir: &Path) -> Result<StopOutcome> {
    let Some(info) = read_daemon_info(aikit_dir) else {
        return Ok(StopOutcome::NotRunning);
    };
    if !probe_alive(info.bind, info.port).await {
        remove_daemon_info(aikit_dir);
        return Ok(StopOutcome::StaleRemoved);
    }
    terminate_pid(info.pid)?;
    for _ in 0..20 {
        if !probe_alive(info.bind, info.port).await {
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

pub fn stop_owned_child(aikit_dir: &Path, child: &mut Child) -> Result<()> {
    kill_and_reap_child(child)?;
    if read_daemon_info(aikit_dir).is_some_and(|info| info.pid == child.id()) {
        remove_daemon_info(aikit_dir);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartOutcome {
    Started {
        pid: u32,
        url: String,
        token: String,
        token_created: bool,
    },
    AlreadyRunning {
        info: DaemonInfo,
    },
}

pub async fn ensure_startable(aikit_dir: &Path) -> Result<Option<DaemonInfo>> {
    if let Some(info) = read_daemon_info(aikit_dir) {
        if probe_alive(info.bind, info.port).await {
            return Ok(Some(info));
        }
        remove_daemon_info(aikit_dir);
    }
    Ok(None)
}

pub async fn start(aikit_dir: &Path, bind: IpAddr, port: u16) -> Result<StartOutcome> {
    let executable = std::env::current_exe().map_err(AikitError::Io)?;
    start_with_executable(aikit_dir, bind, port, &executable).await
}

pub async fn start_with_executable(
    aikit_dir: &Path,
    bind: IpAddr,
    port: u16,
    executable: &Path,
) -> Result<StartOutcome> {
    let (outcome, _) = start_with_executable_owned(aikit_dir, bind, port, executable).await?;
    Ok(outcome)
}

pub async fn start_with_executable_owned(
    aikit_dir: &Path,
    bind: IpAddr,
    port: u16,
    executable: &Path,
) -> Result<(StartOutcome, Option<Child>)> {
    if let Some(info) = ensure_startable(aikit_dir).await? {
        return Ok((StartOutcome::AlreadyRunning { info }, None));
    }

    // Generate the token up front so the CLI can show it (especially on the
    // very first run); the spawned serve process reuses the same file.
    let token_created = crate::token::read_valid_token(aikit_dir).is_none();
    let token = crate::token::ensure_token(aikit_dir)?;

    let mut child = spawn_serve(executable, bind, port)?;
    if let Err(err) =
        wait_for_readiness(&mut child, aikit_dir, bind, port, START_READINESS_TIMEOUT).await
    {
        if read_daemon_info(aikit_dir).is_some_and(|info| info.pid == child.id()) {
            remove_daemon_info(aikit_dir);
        }
        return Err(err);
    }
    Ok((
        StartOutcome::Started {
            pid: child.id(),
            url: format_http_url(bind, port),
            token,
            token_created,
        },
        Some(child),
    ))
}

async fn wait_for_readiness(
    child: &mut Child,
    aikit_dir: &Path,
    bind: IpAddr,
    port: u16,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let result = async {
        loop {
            if let Some(status) = child.try_wait()? {
                return Err(AikitError::Provider(format!(
                    "daemon process exited before becoming ready (code {status}); 端口 {port} 可能被占用"
                )));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(AikitError::Provider(format!(
                    "daemon did not become ready on port {port} within timeout"
                )));
            }
            if tokio::time::timeout(remaining, probe_alive(bind, port))
                .await
                .unwrap_or(false)
            {
                // The health response may come from an unrelated process
                // holding the port while our spawn failed to bind; a ready
                // daemon always records its own pid first.
                if read_daemon_info(aikit_dir).is_none_or(|info| info.pid != child.id()) {
                    return Err(AikitError::Provider(format!(
                        "port {port} is already served by another process"
                    )));
                }
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100).min(remaining)).await;
        }
    }
    .await;
    if let Err(err) = result {
        kill_and_reap_child(child)
            .map_err(|cleanup_error| AikitError::Provider(format!("{err}; {cleanup_error}")))?;
        return Err(err);
    }
    Ok(())
}

fn kill_and_reap_child(child: &mut Child) -> Result<()> {
    if child.try_wait().ok().flatten().is_none() {
        if let Err(kill_error) = child.kill() {
            if child.try_wait().ok().flatten().is_none() {
                return Err(AikitError::Provider(format!(
                    "failed to terminate child: {kill_error}"
                )));
            }
        }
        child.wait().map_err(|wait_error| {
            AikitError::Provider(format!("failed to reap child: {wait_error}"))
        })?;
    }
    Ok(())
}

pub async fn restart(aikit_dir: &Path, bind: IpAddr, port: u16) -> Result<StartOutcome> {
    stop(aikit_dir).await?;
    start(aikit_dir, bind, port).await
}

fn spawn_serve(executable: &Path, bind: IpAddr, port: u16) -> Result<Child> {
    let mut command = Command::new(executable);
    command
        .args([
            "daemon",
            "serve",
            "--port",
            &port.to_string(),
            "--bind",
            &bind.to_string(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }

    command
        .spawn()
        .map_err(|err| AikitError::Provider(format!("failed to spawn daemon: {err}")))
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

#[cfg(test)]
mod update_start_tests {
    use super::*;

    #[test]
    fn sleeping_child() {
        if std::env::var_os("AIKIT_LIFECYCLE_TEST_CHILD").is_some() {
            std::thread::sleep(Duration::from_secs(60));
        }
    }

    #[tokio::test]
    async fn readiness_timeout_terminates_and_reaps_its_child() {
        let directory = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "lifecycle::update_start_tests::sleeping_child"])
            .env("AIKIT_LIFECYCLE_TEST_CHILD", "1")
            .current_dir(directory.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().unwrap();

        let result = wait_for_readiness(
            &mut child,
            directory.path(),
            address.ip(),
            address.port(),
            Duration::from_millis(100),
        )
        .await;

        assert!(result.unwrap_err().to_string().contains("within timeout"));
        let exit_status = child
            .try_wait()
            .unwrap()
            .expect("startup child must be terminated and reaped");
        assert!(!exit_status.success());
        assert_eq!(child.wait().unwrap(), exit_status);
    }

    #[tokio::test]
    async fn readiness_rejects_a_port_served_by_another_process() {
        let directory = tempfile::tempdir().unwrap();
        // A live HTTP responder on the port plays the role of an unrelated
        // process holding it; the spawned child stays alive but never binds.
        let responder = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = responder.local_addr().unwrap();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            if let Ok((mut stream, _)) = responder.accept() {
                let mut buf = [0u8; 512];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
            }
        });
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "lifecycle::update_start_tests::sleeping_child"])
            .env("AIKIT_LIFECYCLE_TEST_CHILD", "1")
            .current_dir(directory.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().unwrap();

        let result = wait_for_readiness(
            &mut child,
            directory.path(),
            address.ip(),
            address.port(),
            Duration::from_secs(2),
        )
        .await;

        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("another process"),
            "unexpected error: {error}"
        );
        assert!(
            child.try_wait().unwrap().is_some(),
            "the spawned child must be terminated, not reported ready"
        );
    }
}
