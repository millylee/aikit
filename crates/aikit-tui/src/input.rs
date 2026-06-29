use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppState, FocusedPane, ModalState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    None,
    Quit,
    RefreshModels,
    ApplySelection,
    CheckUpdates,
}

pub fn handle_key(state: &mut AppState, key: KeyEvent) -> AppAction {
    if state.is_modal_open() {
        if matches!(state.modal_state, ModalState::ImportPrompt { .. }) {
            return match key.code {
                KeyCode::Enter => {
                    if let Err(err) = state.confirm_import_all() {
                        state.set_status(format!("Import failed: {err}"));
                    }
                    AppAction::None
                }
                KeyCode::Esc => {
                    if let Err(err) = state.skip_import_prompt() {
                        state.set_status(format!("Import failed: {err}"));
                    }
                    AppAction::None
                }
                KeyCode::Tab => {
                    if let Err(err) = state.open_import_list() {
                        state.set_status(format!("Import failed: {err}"));
                    }
                    AppAction::None
                }
                _ => AppAction::None,
            };
        }

        if matches!(state.modal_state, ModalState::ImportList { .. }) {
            return match key.code {
                KeyCode::Char(' ') => {
                    state.toggle_import_candidate();
                    AppAction::None
                }
                KeyCode::Down => {
                    state.import_list_next();
                    AppAction::None
                }
                KeyCode::Up => {
                    state.import_list_previous();
                    AppAction::None
                }
                KeyCode::Enter => {
                    if let Err(err) = state.confirm_selected_imports() {
                        state.set_status(format!("Import failed: {err}"));
                    }
                    AppAction::None
                }
                KeyCode::Esc => {
                    if let Err(err) = state.cancel_import_list() {
                        state.set_status(format!("Import failed: {err}"));
                    }
                    AppAction::None
                }
                _ => AppAction::None,
            };
        }

        if matches!(state.modal_state, ModalState::Shortcuts) {
            return match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') => {
                    state.cancel_modal();
                    AppAction::None
                }
                _ => AppAction::None,
            };
        }

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
            KeyCode::Delete => {
                if let Err(err) = state.modal_delete_field() {
                    state.set_status(format!("Modal edit failed: {err}"));
                }
                AppAction::None
            }
            KeyCode::Left => {
                state.modal_move_cursor_left();
                AppAction::None
            }
            KeyCode::Right => {
                state.modal_move_cursor_right();
                AppAction::None
            }
            KeyCode::Home => {
                state.modal_move_cursor_home();
                AppAction::None
            }
            KeyCode::End => {
                state.modal_move_cursor_end();
                AppAction::None
            }
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                if let Err(err) = state.modal_clear_field() {
                    state.set_status(format!("Modal edit failed: {err}"));
                }
                AppAction::None
            }
            KeyCode::Char(ch) => {
                if matches!(key.modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT)
                    && !ch.is_control()
                {
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
            if let Err(err) = state.edit_selected() {
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
        (KeyCode::Char('+'), _) => {
            if let Err(err) = state.open_add_api_key_modal() {
                state.set_status(format!("Open modal failed: {err}"));
            }
            AppAction::None
        }
        (KeyCode::Char('m'), _) => {
            if let Err(err) = state.open_add_model_modal() {
                state.set_status(format!("Open modal failed: {err}"));
            }
            AppAction::None
        }
        (KeyCode::Char('i'), _) => {
            if let Err(err) = state.open_import_prompt() {
                state.set_status(format!("Open modal failed: {err}"));
            }
            AppAction::None
        }
        (KeyCode::Char('?'), _) => {
            state.open_shortcuts_modal();
            AppAction::None
        }
        (KeyCode::Char('u'), _) => AppAction::CheckUpdates,
        (KeyCode::Char('x'), _) => {
            let result = if state.focused_pane == FocusedPane::Selection
                && state.selection_item_is_api_key()
            {
                state.open_delete_api_key_confirmation()
            } else {
                state.set_status("Select an API key to delete");
                Ok(())
            };
            if let Err(err) = result {
                state.set_status(format!("Open modal failed: {err}"));
            }
            AppAction::None
        }
        (KeyCode::Tab, _) => {
            state.focus_next_pane();
            AppAction::None
        }
        (KeyCode::Right, _) | (KeyCode::Char('l'), _) => {
            state.focus_next_pane();
            AppAction::None
        }
        (KeyCode::Left, _) | (KeyCode::Char('h'), _) => {
            state.focus_previous_pane();
            AppAction::None
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
            state.select_next();
            AppAction::None
        }
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
            state.select_previous();
            AppAction::None
        }
        (KeyCode::Char('g'), _) => {
            state.select_first();
            AppAction::None
        }
        (KeyCode::Char('G'), _) => {
            state.select_last();
            AppAction::None
        }
        (KeyCode::Enter, _) => {
            state.activate_selected();
            AppAction::None
        }
        (KeyCode::Char(' '), _) => {
            state.activate_selected();
            AppAction::None
        }
        (KeyCode::Char('r'), _) => AppAction::RefreshModels,
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => AppAction::ApplySelection,
        _ => AppAction::None,
    }
}
