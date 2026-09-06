pub mod api;
pub mod lifecycle;
pub mod token;

use std::{net::IpAddr, path::Path};

use aikit_core::{config::aikit_dir_for_config, Result};

use crate::{api::router, lifecycle::write_daemon_info};

pub const DEFAULT_PORT: u16 = 7654;
pub const DEFAULT_BIND: &str = "127.0.0.1";

pub async fn serve(aikit_dir: &Path, bind: IpAddr, port: u16) -> Result<()> {
    let token = token::ensure_token(aikit_dir)?;
    write_daemon_info(
        aikit_dir,
        &lifecycle::DaemonInfo {
            pid: std::process::id(),
            port,
        },
    )?;

    let app = router(&token);
    let listener = tokio::net::TcpListener::bind((bind, port))
        .await
        .map_err(|err| aikit_core::AikitError::Provider(format!("daemon bind failed: {err}")))?;
    axum::serve(listener, app)
        .await
        .map_err(|err| aikit_core::AikitError::Provider(format!("daemon serve failed: {err}")))
}

pub fn daemon_dir() -> Result<std::path::PathBuf> {
    let config_path = aikit_core::config::default_config_path()?;
    Ok(aikit_dir_for_config(&config_path))
}
