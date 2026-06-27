use std::io::{self, stdout};

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

fn main() -> Result<()> {
    color_eyre::install()?;
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut state = AppState::default();
    let result = run_app(&mut terminal, &mut state);

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, state: &mut AppState) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::render(frame, state))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match handle_key(state, key) {
                        AppAction::None => {}
                        AppAction::Quit => break,
                        AppAction::RefreshModels => state.set_status("Refresh requested"),
                        AppAction::ApplySelection => state.set_status("Apply requested"),
                    }
                }
            }
        }
    }

    Ok(())
}
