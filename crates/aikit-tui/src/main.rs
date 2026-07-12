use std::io::{self, stdout};

use aikit_core::{
    config::default_config_path,
    import::candidate_fingerprint,
    provider::OpenAiCompatibleClient,
    updater::{self, StageUpdateOutcome},
};
use aikit_tui::app::{format_refresh_error, AppState};
use aikit_tui::input::{handle_key, AppAction};
use aikit_tui::ui;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/millylee/aikit/releases/latest";

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

    if std::env::args().any(|arg| arg == "--version" || arg == "-V") {
        println!("aikit {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let _guard = TerminalGuard::enter()?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut state = AppState::new(default_config_path()?);
    state.load_config()?;
    if let Some(version) = state.apply_pending_update_on_startup()? {
        state.set_status(format!("Installed update v{version}"));
    }
    let http_client = reqwest::Client::new();
    if state.config.providers.is_empty() {
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
    if !state.is_modal_open() && state.should_stage_background_update() {
        let client = http_client.clone();
        let skipped = state.config.update_prompt.skipped_version.clone();
        let config_path = state.config_path.clone();
        tokio::spawn(async move {
            let aikit_dir = aikit_core::config::aikit_dir_for_config(&config_path);
            let result = updater::stage_update_if_available(
                &client,
                LATEST_RELEASE_URL,
                &aikit_dir,
                skipped.as_deref(),
            )
            .await;
            let _ = update_tx.send(result);
        });
    }

    let client = OpenAiCompatibleClient::new(http_client.clone());
    run_app(
        &mut terminal,
        &mut state,
        &client,
        &http_client,
        &mut update_rx,
    )
    .await
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    client: &OpenAiCompatibleClient,
    http_client: &reqwest::Client,
    update_rx: &mut tokio::sync::mpsc::UnboundedReceiver<
        Result<StageUpdateOutcome, aikit_core::AikitError>,
    >,
) -> Result<()> {
    loop {
        while let Ok(result) = update_rx.try_recv() {
            match result {
                Ok(outcome) => {
                    if let Err(err) = state.apply_stage_update_outcome(outcome) {
                        state.set_status(format!("Update staging failed: {err}"));
                    }
                }
                Err(err) => state.set_status(format!("Update check failed: {err}")),
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
                            match state
                                .check_and_stage_updates(http_client, LATEST_RELEASE_URL)
                                .await
                            {
                                Ok(()) => {}
                                Err(err) => state.set_status(format!("Update check failed: {err}")),
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
