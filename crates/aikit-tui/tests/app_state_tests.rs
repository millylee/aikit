use aikit_tui::{
    app::{AppState, FocusedPane},
    input::{handle_key, AppAction},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn tab_moves_focus_between_three_panes() {
    let mut state = AppState::default();
    assert_eq!(state.focused_pane, FocusedPane::Providers);

    let action = handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    assert_eq!(action, AppAction::None);
    assert_eq!(state.focused_pane, FocusedPane::Details);
}

#[test]
fn ctrl_s_requests_apply() {
    let mut state = AppState::default();
    let action = handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
    );

    assert_eq!(action, AppAction::ApplySelection);
}

#[test]
fn r_requests_model_refresh() {
    let mut state = AppState::default();
    let action = handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
    );

    assert_eq!(action, AppAction::RefreshModels);
}
