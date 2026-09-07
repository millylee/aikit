use std::{
    fmt::Display,
    io::{self, stdout},
    net::IpAddr,
    path::Path,
};

use aikit_core::{
    config::{aikit_dir_for_config, default_config_path},
    import::candidate_fingerprint,
    provider::OpenAiCompatibleClient,
    updater::{self, LATEST_RELEASE_URL},
    AikitError,
};
use aikit_tui::app::{format_refresh_error, AppState};
use aikit_tui::input::{handle_key, AppAction};
use aikit_tui::ui;
use aikit_tui::update::{self, StartupHandoff, UpdateEvent};
use clap::{Parser, Subcommand};
use color_eyre::Result;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::Terminal;

#[derive(Parser)]
#[command(
    name = "aikit",
    version,
    about = "管理 OpenAI 兼容供应商并应用到 AI 工具配置"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// 管理 Web UI 后台服务
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    /// 启动后台服务
    Start {
        #[arg(long, default_value_t = aikit_daemon::DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = aikit_daemon::DEFAULT_BIND)]
        bind: IpAddr,
    },
    /// 重启后台服务
    Restart {
        #[arg(long, default_value_t = aikit_daemon::DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = aikit_daemon::DEFAULT_BIND)]
        bind: IpAddr,
    },
    /// 停止后台服务
    Stop,
    /// 查看后台服务运行状态
    Status,
    /// 前台运行服务进程
    Serve {
        #[arg(long, default_value_t = aikit_daemon::DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = aikit_daemon::DEFAULT_BIND)]
        bind: IpAddr,
    },
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        if let Err(err) = stdout().execute(EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(err.into());
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();
    match cli.command {
        None => run_tui().await,
        Some(Command::Daemon { action }) => run_daemon(action).await,
    }
}

async fn run_daemon(action: DaemonAction) -> Result<()> {
    let dir = aikit_daemon::daemon_dir()?;
    match action {
        DaemonAction::Serve { port, bind } => {
            aikit_daemon::serve(&dir, bind, port).await?;
            Ok(())
        }
        DaemonAction::Status => {
            match aikit_daemon::lifecycle::status(&dir).await? {
                aikit_daemon::lifecycle::StatusReport::Running { info } => println!(
                    "运行中：pid {}，端口 {}（{}）",
                    info.pid,
                    info.port,
                    aikit_daemon::lifecycle::format_http_url(info.bind, info.port)
                ),
                aikit_daemon::lifecycle::StatusReport::Stale { info } => println!(
                    "已停止（发现残留记录：pid {}，端口 {}）",
                    info.pid, info.port
                ),
                aikit_daemon::lifecycle::StatusReport::Stopped => println!("已停止"),
            }
            Ok(())
        }
        DaemonAction::Stop => {
            match aikit_daemon::lifecycle::stop(&dir).await? {
                aikit_daemon::lifecycle::StopOutcome::Stopped { pid } => {
                    println!("已停止后台服务（pid {pid}）")
                }
                aikit_daemon::lifecycle::StopOutcome::NotRunning => {
                    println!("后台服务未在运行")
                }
                aikit_daemon::lifecycle::StopOutcome::StaleRemoved => {
                    println!("后台服务未在运行，已清理残留记录")
                }
            }
            Ok(())
        }
        DaemonAction::Start { port, bind } => {
            report_start(
                aikit_daemon::lifecycle::start(&dir, bind, port).await?,
                &dir,
            );
            Ok(())
        }
        DaemonAction::Restart { port, bind } => {
            report_start(
                aikit_daemon::lifecycle::restart(&dir, bind, port).await?,
                &dir,
            );
            Ok(())
        }
    }
}

fn report_start(outcome: aikit_daemon::lifecycle::StartOutcome, dir: &std::path::Path) {
    match outcome {
        aikit_daemon::lifecycle::StartOutcome::Started {
            pid,
            url,
            token,
            token_created,
        } => {
            println!("后台服务已启动：pid {pid}（{url}）");
            if token_created {
                println!("首次生成访问令牌：{token}");
                println!(
                    "登录 Web UI 时输入它即可；如需更换，可编辑 {}（重启后台服务后生效）",
                    aikit_daemon::token::token_path(dir).display()
                );
            } else {
                println!(
                    "访问令牌文件：{}",
                    aikit_daemon::token::token_path(dir).display()
                );
            }
        }
        aikit_daemon::lifecycle::StartOutcome::AlreadyRunning { info } => println!(
            "后台服务已在运行：pid {}（{}）",
            info.pid,
            aikit_daemon::lifecycle::format_http_url(info.bind, info.port)
        ),
    }
}

async fn run_tui() -> Result<()> {
    let mut state = AppState::new(default_config_path()?);
    state.load_config()?;
    let aikit_dir = aikit_dir_for_config(&state.config_path);
    let handoff = StartupHandoff::from_environment(&aikit_dir, env!("CARGO_PKG_VERSION"))?;
    let mut startup_update_failed = false;
    if handoff.is_none() {
        let installation = match update::pending_version_for_startup(&state) {
            Ok(Some(version)) => {
                println!("正在安装更新 v{version}…");
                launch_pending_update(&state, &version).await.map(Some)
            }
            Ok(None) => {
                if let Ok(executable) = std::env::current_exe() {
                    let _ = updater::cleanup_previous_binary(&executable);
                }
                Ok(None)
            }
            Err(err) => Err(err.into()),
        };
        match installation {
            Ok(Some((mut child, _guard))) => {
                let status = tokio::task::spawn_blocking(move || child.wait())
                    .await
                    .map_err(|err| AikitError::Provider(format!("等待新版 aikit 进程失败：{err}")))?
                    .map_err(AikitError::Io)?;
                if !status.success() {
                    color_eyre::eyre::bail!("新版 aikit 已退出：{status}");
                }
                return Ok(());
            }
            Ok(None) => {}
            Err(err) => {
                startup_update_failed = true;
                state.set_status(startup_update_failure_status(&err, &aikit_dir));
            }
        }
    }

    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let http_client = reqwest::Client::new();
    if handoff.is_none() && !startup_update_failed && state.config.providers.is_empty() {
        let plan = state.scan_import_candidates();
        if !plan.candidates.is_empty() {
            let fingerprint = candidate_fingerprint(&plan.candidates);
            let skipped = state.config.import_prompt.skipped_fingerprint.as_deref();
            if skipped != Some(fingerprint.as_str()) {
                state.open_startup_import_prompt_from_plan(plan)?;
            }
        }
    }

    let (update_tx, mut update_rx) = tokio::sync::mpsc::unbounded_channel();
    if let Some(receipt) = handoff {
        state.set_status(format!("已更新至 v{}", receipt.version()));
        terminal.draw(|frame| ui::render(frame, &state))?;
        receipt.acknowledge(&mut state)?;
        if let Err(err) = updater::clear_pending_update(&aikit_dir) {
            state.set_status(format!(
                "已更新至 v{}，下载文件暂未清理：{err}",
                receipt.version()
            ));
        }
    } else if !startup_update_failed
        && !state.is_modal_open()
        && state.should_stage_background_update()
        && state.begin_update_check()
    {
        start_update_check(
            &mut terminal,
            &state,
            &http_client,
            LATEST_RELEASE_URL,
            &update_tx,
        )?;
    }

    let client = OpenAiCompatibleClient::new(http_client.clone());
    run_app(
        &mut terminal,
        &mut state,
        &client,
        &http_client,
        &update_tx,
        &mut update_rx,
    )
    .await
}

fn startup_update_failure_status(error: impl Display, aikit_dir: &Path) -> String {
    format!(
        "更新安装失败，已保留下载文件（{}）；删除该目录即可取消本次更新，或稍后重试：{error}",
        updater::pending_update_dir(aikit_dir).display()
    )
}

async fn launch_pending_update(
    state: &AppState,
    version: &str,
) -> Result<(std::process::Child, TerminalGuard)> {
    let executable = std::env::current_exe()?;
    let prepared = update::prepare_installation(&state.config_path, version, &executable).await?;
    let arguments = std::env::args_os().skip(1).collect();
    Ok(prepared
        .launch(arguments, || {
            TerminalGuard::enter()
                .map_err(|err| AikitError::Provider(format!("终端初始化失败：{err}")))
        })
        .await?)
}

fn start_update_check<RenderBackend>(
    terminal: &mut Terminal<RenderBackend>,
    state: &AppState,
    http_client: &reqwest::Client,
    latest_release_url: &str,
    update_tx: &tokio::sync::mpsc::UnboundedSender<UpdateEvent>,
) -> Result<()>
where
    RenderBackend: Backend,
    RenderBackend::Error: Send + Sync + 'static,
{
    terminal.draw(|frame| ui::render(frame, state))?;
    let _worker = update::spawn_update_check(
        http_client.clone(),
        latest_release_url.to_string(),
        aikit_dir_for_config(&state.config_path),
        state.config.update_prompt.skipped_version.clone(),
        update_tx.clone(),
    );
    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    client: &OpenAiCompatibleClient,
    http_client: &reqwest::Client,
    update_tx: &tokio::sync::mpsc::UnboundedSender<UpdateEvent>,
    update_rx: &mut tokio::sync::mpsc::UnboundedReceiver<UpdateEvent>,
) -> Result<()> {
    loop {
        while let Ok(event) = update_rx.try_recv() {
            match event {
                UpdateEvent::Downloading(version) => state.mark_update_downloading(&version),
                UpdateEvent::Finished(outcome) => {
                    if let Err(err) = state.finish_update_check(outcome) {
                        state.set_status(format!("更新检查或下载失败：{err}"));
                    }
                }
            }
        }

        terminal.draw(|frame| ui::render(frame, state))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match handle_key(state, key) {
                        AppAction::None => {}
                        AppAction::Quit => break,
                        AppAction::RefreshModels => match state.refresh_active_models(client).await
                        {
                            Ok(outcome) => state.set_status(outcome.message),
                            Err(err) => state.set_status(format_refresh_error(&err)),
                        },
                        AppAction::ApplySelection => match state.apply_active_selection() {
                            Ok(outcome) => state.set_status(outcome.message),
                            Err(err) => state.set_status(format!("Apply failed: {err}")),
                        },
                        AppAction::CheckUpdates => {
                            start_update_check(
                                terminal,
                                state,
                                http_client,
                                LATEST_RELEASE_URL,
                                update_tx,
                            )?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn update_request_draws_checking_status_before_waiting_for_the_network() {
        let directory = tempfile::tempdir().unwrap();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/releases/latest"))
            .respond_with(
                ResponseTemplate::new(500).set_delay(std::time::Duration::from_millis(300)),
            )
            .expect(1)
            .mount(&server)
            .await;
        let mut terminal = Terminal::new(TestBackend::new(160, 40)).unwrap();
        let mut state = AppState::new(directory.path().join("config.toml"));
        let action = handle_key(
            &mut state,
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('u'),
                crossterm::event::KeyModifiers::NONE,
            ),
        );
        assert_eq!(action, AppAction::CheckUpdates);
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();

        start_update_check(
            &mut terminal,
            &state,
            &reqwest::Client::new(),
            &format!("{}/releases/latest", server.uri()),
            &sender,
        )
        .unwrap();

        for (index, symbol) in "正在检查更新".chars().enumerate() {
            assert_eq!(
                terminal.backend().buffer()[(index as u16 * 2, 39)].symbol(),
                symbol.to_string()
            );
        }
        assert!(state.update_in_progress);
        assert!(receiver.try_recv().is_err());
        let event = tokio::time::timeout(std::time::Duration::from_secs(5), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            event,
            aikit_tui::update::UpdateEvent::Finished(Err(_))
        ));
    }

    #[test]
    fn startup_update_failure_status_names_the_pending_directory_and_how_to_cancel() {
        let directory = tempfile::tempdir().unwrap();
        let status =
            startup_update_failure_status(AikitError::Provider("boom".into()), directory.path());

        assert!(status.contains("boom"));
        let pending = directory.path().join("pending-update");
        assert!(
            status.contains(&pending.display().to_string()),
            "missing pending directory in: {status}"
        );
        assert!(status.contains("删除"), "missing cancel hint in: {status}");
    }
}
