use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    None,
    Quit,
    RefreshModels,
    ApplySelection,
}

pub fn handle_key(state: &mut AppState, key: KeyEvent) -> AppAction {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => AppAction::Quit,
        (KeyCode::Tab, _) => {
            state.focus_next_pane();
            AppAction::None
        }
        (KeyCode::Down, _) => {
            state.select_next();
            AppAction::None
        }
        (KeyCode::Up, _) => {
            state.select_previous();
            AppAction::None
        }
        (KeyCode::Enter, _) => {
            state.activate_selected();
            AppAction::None
        }
        (KeyCode::Char(' '), _) => {
            state.toggle_selected_target();
            AppAction::None
        }
        (KeyCode::Char('r'), _) => AppAction::RefreshModels,
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => AppAction::ApplySelection,
        _ => AppAction::None,
    }
}
