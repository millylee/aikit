use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppState, FocusedPane};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    None,
    Quit,
    RefreshModels,
    ApplySelection,
}

pub fn handle_key(state: &mut AppState, key: KeyEvent) -> AppAction {
    if state.is_modal_open() {
        return match key.code {
            KeyCode::Esc => {
                state.cancel_modal();
                AppAction::None
            }
            KeyCode::Tab => {
                state.modal_next_field();
                AppAction::None
            }
            KeyCode::BackTab => {
                state.modal_previous_field();
                AppAction::None
            }
            KeyCode::Enter => {
                let result = if state.modal_is_confirmation() {
                    state.confirm_modal()
                } else {
                    state.save_modal()
                };
                if let Err(err) = result {
                    state.set_status(format!("Modal failed: {err}"));
                }
                AppAction::None
            }
            KeyCode::Backspace => {
                if let Err(err) = state.modal_backspace_field() {
                    state.set_status(format!("Modal edit failed: {err}"));
                }
                AppAction::None
            }
            KeyCode::Char(ch) => {
                if !ch.is_control() {
                    if let Err(err) = state.modal_append_char(ch) {
                        state.set_status(format!("Modal edit failed: {err}"));
                    }
                }
                AppAction::None
            }
            _ => AppAction::None,
        };
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => AppAction::Quit,
        (KeyCode::Char('a'), _) => {
            state.open_add_provider_modal();
            AppAction::None
        }
        (KeyCode::Char('e'), _) => {
            let result = if state.focused_pane == FocusedPane::Details {
                state.open_edit_api_key_modal()
            } else {
                state.open_edit_provider_modal()
            };
            if let Err(err) = result {
                state.set_status(format!("Open modal failed: {err}"));
            }
            AppAction::None
        }
        (KeyCode::Char('d'), _) => {
            if let Err(err) = state.open_delete_provider_confirmation() {
                state.set_status(format!("Open modal failed: {err}"));
            }
            AppAction::None
        }
        (KeyCode::Char('k'), _) => {
            if let Err(err) = state.open_add_api_key_modal() {
                state.set_status(format!("Open modal failed: {err}"));
            }
            AppAction::None
        }
        (KeyCode::Char('x'), _) => {
            if let Err(err) = state.open_delete_api_key_confirmation() {
                state.set_status(format!("Open modal failed: {err}"));
            }
            AppAction::None
        }
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
