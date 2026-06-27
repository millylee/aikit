use std::io::{self, stdout};

use aikit_core::{
    config::default_config_path, import::candidate_fingerprint, provider::OpenAiCompatibleClient,
};
use aikit_tui::app::AppState;
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
    if state.config.providers.is_empty() {
        let plan = state.scan_import_candidates();
        if !plan.candidates.is_empty() {
            let fingerprint = candidate_fingerprint(&plan.candidates);
            let skipped = state.config.import_prompt.skipped_fingerprint.as_deref();
            if skipped != Some(fingerprint.as_str()) {
                state.open_import_prompt_from_plan(plan)?;
            }
        }
    }
    let client = OpenAiCompatibleClient::new(reqwest::Client::new());
    run_app(&mut terminal, &mut state, &client).await
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    client: &OpenAiCompatibleClient,
) -> Result<()> {
    loop {
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
                            Err(err) => state.set_status(format!("Refresh failed: {err}")),
                        },
                        AppAction::ApplySelection => match state.apply_active_selection() {
                            Ok(outcome) => state.set_status(outcome.message),
                            Err(err) => state.set_status(format!("Apply failed: {err}")),
                        },
                    }
                }
            }
        }
    }

    Ok(())
}
