pub mod api;
pub mod lifecycle;
pub mod token;

use std::{net::IpAddr, path::Path};

use aikit_core::{config::aikit_dir_for_config, AikitError, Result};

use crate::{
    api::router,
    lifecycle::{write_daemon_info, DaemonInfo},
};

pub const DEFAULT_PORT: u16 = 7654;
pub const DEFAULT_BIND: &str = "127.0.0.1";

pub async fn serve(aikit_dir: &Path, bind: IpAddr, port: u16) -> Result<()> {
    let token = token::ensure_token(aikit_dir)?;
    let app = router(&token);

    let listener = tokio::net::TcpListener::bind((bind, port))
        .await
        .map_err(|err| AikitError::Provider(format!("daemon bind failed: {err}")))?;
    write_daemon_info(
        aikit_dir,
        &DaemonInfo {
            pid: std::process::id(),
            port,
            bind,
        },
    )?;

    let server = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal());
    let serve_result = server
        .await
        .map_err(|err| AikitError::Provider(format!("daemon serve failed: {err}")));

    if lifecycle::read_daemon_info(aikit_dir).is_some_and(|info| info.pid == std::process::id()) {
        lifecycle::remove_daemon_info(aikit_dir);
    }
    serve_result
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

pub fn daemon_dir() -> Result<std::path::PathBuf> {
    let config_path = aikit_core::config::default_config_path()?;
    Ok(aikit_dir_for_config(&config_path))
}
