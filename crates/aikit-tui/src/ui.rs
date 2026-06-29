use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use aikit_core::import::{ImportCandidate, ImportSource};

use crate::app::{ApiKeyFormMode, AppState, FocusedPane, ModalState, ProviderFormMode};

pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let panes_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(main_layout[0]);

    let providers = Paragraph::new(providers_text(state)).block(pane_block(
        "Providers",
        state.focused_pane == FocusedPane::Providers,
    ));
    let details = Paragraph::new(selection_text(state)).block(pane_block(
        "Selection",
        state.focused_pane == FocusedPane::Selection,
    ));
    let targets = Paragraph::new(targets_text(state)).block(pane_block(
        "Apply To",
        state.focused_pane == FocusedPane::ApplyTo,
    ));

    frame.render_widget(providers, panes_layout[0]);
    frame.render_widget(details, panes_layout[1]);
    frame.render_widget(targets, panes_layout[2]);

    let footer_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(status_footer_hint().chars().count() as u16),
        ])
        .split(main_layout[1]);
    let status = Paragraph::new(state.status.as_str());
    let hint = Paragraph::new(status_footer_hint()).alignment(Alignment::Right);
    frame.render_widget(status, footer_layout[0]);
    frame.render_widget(hint, footer_layout[1]);

    render_modal(frame, state);
}

fn pane_block(title: &str, focused: bool) -> Block<'static> {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(pane_title(title, focused));
    if focused {
        block
            .border_style(focused_pane_style())
            .title_style(focused_pane_style())
    } else {
        block
    }
}

fn focused_pane_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn pane_title(title: &str, focused: bool) -> String {
    if focused {
        format!("> {title}")
    } else {
        title.to_string()
    }
}

fn providers_text(state: &AppState) -> String {
    if state.config.providers.is_empty() {
        return "No providers configured\nAdd providers in the config file, then restart or reload."
            .into();
    }

    let active_provider_id = state
        .config
        .active_selection
        .as_ref()
        .map(|selection| selection.provider_id.as_str());
    state
        .config
        .providers
        .iter()
        .enumerate()
        .map(|(index, provider)| {
            let cursor = if index == state.provider_index {
                ">"
            } else {
                " "
            };
            let active = if Some(provider.id.as_str()) == active_provider_id {
                "*"
            } else {
                " "
            };
            format!("{cursor}{active} {}", provider.name)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn selection_text(state: &AppState) -> String {
    let Some(provider) = state.selected_provider() else {
        return "Select a provider.".into();
    };
    let active = state.config.active_selection.as_ref();
    let mut lines = vec![
        format!("Provider: {}", provider.name),
        format!("Base URL: {}", provider.base_url),
        "API Key:".into(),
    ];

    if provider.api_keys.is_empty() {
        let cursor = if state.detail_index() == 0 { ">" } else { " " };
        lines.push(format!("{cursor} Add API key (+)"));
    } else {
        for (index, key) in provider.api_keys.iter().enumerate() {
            let cursor = if state.detail_index() == index {
                ">"
            } else {
                " "
            };
            let active_key = active
                .filter(|selection| {
                    selection.provider_id == provider.id && selection.api_key_id == key.id
                })
                .map(|_| "*")
                .unwrap_or(" ");
            lines.push(format!(
                "{cursor}{active_key} {} ({})",
                key.name,
                mask_secret(&key.value)
            ));
        }
    }

    lines.push(String::new());
    lines.push("Model:".into());
    let key_count = provider.api_keys.len();
    let model_start_index = if provider.api_keys.is_empty() {
        1
    } else {
        key_count
    };
    let cached_models = provider
        .models_cache
        .as_ref()
        .map(|cache| cache.models.as_slice())
        .unwrap_or(&[]);
    if cached_models.is_empty() && provider.manual_models.is_empty() {
        let cursor = if state.detail_index() == model_start_index {
            ">"
        } else {
            " "
        };
        lines.push(format!("{cursor} Add model (m)"));
    } else {
        for (index, model) in cached_models.iter().enumerate() {
            let detail_index = model_start_index + index;
            let cursor = if state.detail_index() == detail_index {
                ">"
            } else {
                " "
            };
            let active_model = active
                .filter(|selection| {
                    selection.provider_id == provider.id && selection.model_id == *model
                })
                .map(|_| "*")
                .unwrap_or(" ");
            lines.push(format!("{cursor}{active_model} {model}"));
        }
        for (index, model) in provider.manual_models.iter().enumerate() {
            let detail_index = model_start_index + cached_models.len() + index;
            let cursor = if state.detail_index() == detail_index {
                ">"
            } else {
                " "
            };
            let active_model = active
                .filter(|selection| {
                    selection.provider_id == provider.id && selection.model_id == *model
                })
                .map(|_| "*")
                .unwrap_or(" ");
            lines.push(format!("{cursor}{active_model} {model}  manual"));
        }
    }

    lines.join("\n")
}

fn targets_text(state: &AppState) -> String {
    if state.config.targets.is_empty() {
        return "No targets configured.".into();
    }

    state
        .config
        .targets
        .iter()
        .enumerate()
        .map(|(index, target)| {
            let cursor = if index == state.target_index {
                ">"
            } else {
                " "
            };
            let enabled = if target.enabled { "[x]" } else { "[ ]" };
            format!("{cursor} {enabled} {}", target_display_name(&target.id))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn target_display_name(target_id: &str) -> &str {
    match target_id {
        "claude" => "Claude Code",
        "gemini" => "Gemini CLI",
        "codex" => "Codex CLI",
        other => other,
    }
}

fn render_modal(frame: &mut Frame, state: &AppState) {
    let area = centered_rect(70, 50, frame.area());
    match &state.modal_state {
        ModalState::None => {}
        ModalState::ProviderForm(form) => {
            let title = match form.mode {
                ProviderFormMode::Add => "Add Provider",
                ProviderFormMode::Edit { .. } => "Edit Provider",
            };
            let mut lines = vec![
                format!(
                    "{} name*: {}",
                    field_cursor(form.current_field == 0),
                    input_box(form.name.as_str())
                ),
                format!(
                    "{} base_url*: {}",
                    field_cursor(form.current_field == 1),
                    input_box(form.base_url.as_str())
                ),
                format!(
                    "{} model*: {}",
                    field_cursor(form.current_field == 2),
                    input_box(form.model.as_str())
                ),
                String::new(),
                "* required. Tab/Shift+Tab switch field.".into(),
                "Left/Right/Home/End edit, Ctrl+U clear, Enter save, Esc cancel.".into(),
            ];
            if let Some(error) = &form.validation_error {
                lines.push(format!("Error: {error}"));
            }
            render_modal_text(frame, area, title, lines.join("\n"));
        }
        ModalState::ApiKeyForm(form) => {
            let title = match form.mode {
                ApiKeyFormMode::Add => "Add API Key",
                ApiKeyFormMode::Edit { .. } => "Edit API Key",
            };
            let mut lines = match form.mode {
                ApiKeyFormMode::Add => vec![
                    format!(
                        "{} value*: {}",
                        field_cursor(true),
                        input_box(form.value.as_str())
                    ),
                    String::new(),
                    "* required. Left/Right/Home/End edit.".into(),
                    "Ctrl+U clear, Enter save, Esc cancel.".into(),
                ],
                ApiKeyFormMode::Edit { .. } => vec![
                    format!(
                        "{} name*: {}",
                        field_cursor(form.current_field == 0),
                        input_box(form.name.as_str())
                    ),
                    format!(
                        "{} value*: {}",
                        field_cursor(form.current_field == 1),
                        input_box(form.value.as_str())
                    ),
                    String::new(),
                    "* required. Tab/Shift+Tab switch field.".into(),
                    "Left/Right/Home/End edit, Ctrl+U clear, Enter save, Esc cancel.".into(),
                ],
            };
            if let Some(error) = &form.validation_error {
                lines.push(format!("Error: {error}"));
            }
            render_modal_text(frame, area, title, lines.join("\n"));
        }
        ModalState::ModelForm(form) => {
            let mut lines = vec![
                format!(
                    "{} model*: {}",
                    field_cursor(true),
                    input_box(form.model.as_str())
                ),
                String::new(),
                "* required. Left/Right/Home/End edit.".into(),
                "Ctrl+U clear, Enter save, Esc cancel.".into(),
            ];
            if let Some(error) = &form.validation_error {
                lines.push(format!("Error: {error}"));
            }
            render_modal_text(frame, area, "Add Model", lines.join("\n"));
        }
        ModalState::ConfirmDeleteProvider { provider_id } => {
            render_modal_text(
                frame,
                area,
                "Delete Provider",
                format!(
                    "Delete provider `{provider_id}`?\nThis will remove all API keys.\n\nEnter confirm, Esc cancel"
                ),
            );
        }
        ModalState::ConfirmDeleteApiKey {
            provider_id,
            api_key_id,
        } => {
            render_modal_text(
                frame,
                area,
                "Delete API Key",
                format!(
                    "Delete API key `{api_key_id}` from provider `{provider_id}`?\n\nEnter confirm, Esc cancel"
                ),
            );
        }
        ModalState::ImportPrompt {
            candidates,
            warnings,
            ..
        } => {
            let mut lines = vec![
                format!("Found {} import candidate(s):", candidates.len()),
                String::new(),
            ];
            lines.extend(candidates.iter().map(format_import_candidate));
            if !warnings.is_empty() {
                lines.push(String::new());
                lines.push("Warnings:".into());
                lines.extend(warnings.iter().map(|warning| format!("- {warning}")));
            }
            lines.push(String::new());
            lines.push("Enter import all, Tab/l select candidates, Esc skip".into());
            render_modal_text(frame, area, "Import Providers", lines.join("\n"));
        }
        ModalState::ImportList {
            candidates,
            selected_indices,
            cursor,
            warnings,
            ..
        } => {
            let mut lines = vec!["Select candidates to import:".into(), String::new()];
            for (index, candidate) in candidates.iter().enumerate() {
                let cursor_mark = if index == *cursor { ">" } else { " " };
                let selected = selected_indices.get(index).copied().unwrap_or(false);
                let selected_mark = if selected { "[x]" } else { "[ ]" };
                lines.push(format!(
                    "{cursor_mark}{selected_mark} {}",
                    format_import_candidate(candidate)
                ));
            }
            if !warnings.is_empty() {
                lines.push(String::new());
                lines.push("Warnings:".into());
                lines.extend(warnings.iter().map(|warning| format!("- {warning}")));
            }
            lines.push(String::new());
            lines.push("Space toggle, Up/Down move, Enter import selected, Esc cancel".into());
            render_modal_text(frame, area, "Import Candidates", lines.join("\n"));
        }
        ModalState::Shortcuts => {
            render_modal_text(frame, area, "Shortcuts", shortcuts_text());
        }
    }
}

fn render_modal_text(frame: &mut Frame, area: Rect, title: &str, text: String) {
    frame.render_widget(Clear, area);
    let widget = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(title))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn field_cursor(active: bool) -> &'static str {
    if active {
        ">"
    } else {
        " "
    }
}

fn input_box(value: &str) -> String {
    format!("[ {value} ]")
}

fn status_footer_hint() -> String {
    format!(
        "aikit {} / [?] Shortcuts / [u] Update",
        env!("CARGO_PKG_VERSION")
    )
}

fn shortcuts_text() -> String {
    [
        "Global:",
        "  Tab: switch panes",
        "  t: focus Apply To",
        "  Up/Down or k/j: move selection",
        "  Enter/Space: activate selected item or toggle selected target",
        "  ?: show shortcuts",
        "  u: check for updates",
        "  q/Esc: quit",
        "",
        "Providers / Selection / Apply To:",
        "  a: add provider",
        "  +: add API key",
        "  m: add manual model",
        "  e: edit selected provider, API key, or model",
        "  x: delete selected API key in Selection",
        "  r: refresh models",
        "  Ctrl+s: apply active selection to enabled targets",
        "",
        "Modal forms:",
        "  Tab/Shift+Tab: switch input fields",
        "  Left/Right/Home/End: move within input",
        "  Backspace/Delete: delete before or at cursor",
        "  Ctrl+U: clear current input",
        "  Enter: save or confirm",
        "  Esc: cancel",
    ]
    .join("\n")
}

fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() < 9 {
        return "***".into();
    }
    let prefix = chars.iter().take(4).collect::<String>();
    let suffix = chars[chars.len() - 4..].iter().collect::<String>();
    format!("{prefix}...{suffix}")
}

fn format_import_candidate(candidate: &ImportCandidate) -> String {
    let source = import_source_name(candidate.source);
    let base_url = candidate.base_url.as_deref().unwrap_or("-");
    let model = candidate.model.as_deref().unwrap_or("-");
    let key_preview = candidate
        .api_key_value
        .as_deref()
        .map(mask_secret)
        .unwrap_or_else(|| "-".into());
    format!(
        "{} ({source}) key: {key_preview} base_url: {base_url} model: {model}",
        candidate.provider_name
    )
}

fn import_source_name(source: ImportSource) -> &'static str {
    match source {
        ImportSource::Env => "env",
        ImportSource::Claude => "claude",
        ImportSource::Gemini => "gemini",
        ImportSource::Codex => "codex",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use aikit_core::config::{
        ActiveSelection, AikitConfig, ApiKeyConfig, ModelCache, ProviderConfig, TargetConfig,
    };

    use super::{providers_text, selection_text, shortcuts_text, status_footer_hint, targets_text};
    use crate::app::AppState;

    #[test]
    fn provider_pane_is_single_line_without_model_count() {
        let state = AppState::from_config(PathBuf::from("config.toml"), sample_config());

        let text = providers_text(&state);

        assert!(text.contains("* Provider"));
        assert!(!text.contains("model(s)"));
        assert!(!text.contains("enabled"));
    }

    #[test]
    fn selection_pane_shows_masked_key_and_hides_cache_metadata() {
        let state = AppState::from_config(PathBuf::from("config.toml"), sample_config());

        let text = selection_text(&state);

        assert!(text.contains("Provider: Provider"));
        assert!(text.contains("Base URL: https://example.com/v1"));
        assert!(text.contains("Key (sk-a...7890)"));
        assert!(text.contains("manual-model  manual"));
        assert!(!text.contains("Cache refreshed"));
        assert!(!text.contains("Last refresh error"));
    }

    #[test]
    fn targets_pane_only_shows_enabled_state_and_target_id() {
        let state = AppState::from_config(PathBuf::from("config.toml"), sample_config());

        let text = targets_text(&state);

        assert!(text.contains("> [x] Claude Code"));
        assert!(text.contains("  [ ] Gemini CLI"));
        assert!(!text.contains("default path"));
        assert!(!text.contains("not applied"));
    }

    #[test]
    fn footer_hint_shows_version_and_shortcut_prompts() {
        let text = status_footer_hint();

        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains("[?] Shortcuts"));
        assert!(text.contains("[u] Update"));
    }

    #[test]
    fn shortcuts_text_includes_update_and_modal_keys() {
        let text = shortcuts_text();

        assert!(text.contains("?: show shortcuts"));
        assert!(text.contains("u: check for updates"));
        assert!(text.contains("Ctrl+U: clear current input"));
    }

    fn sample_config() -> AikitConfig {
        AikitConfig {
            providers: vec![ProviderConfig {
                id: "provider".into(),
                name: "Provider".into(),
                base_url: "https://example.com/v1".into(),
                enabled: true,
                api_keys: vec![ApiKeyConfig {
                    id: "key".into(),
                    name: "Key".into(),
                    value: "sk-abcdef1234567890".into(),
                }],
                manual_models: vec!["manual-model".into()],
                models_cache: Some(ModelCache {
                    refreshed_at: "2026-06-28T00:00:00Z".into(),
                    models: vec!["cached-model".into()],
                    last_error: Some("hidden".into()),
                }),
            }],
            active_selection: Some(ActiveSelection {
                provider_id: "provider".into(),
                api_key_id: "key".into(),
                model_id: "manual-model".into(),
            }),
            targets: vec![
                TargetConfig {
                    id: "claude".into(),
                    enabled: true,
                    config_path: None,
                },
                TargetConfig {
                    id: "gemini".into(),
                    enabled: false,
                    config_path: None,
                },
            ],
            ..AikitConfig::default()
        }
    }
}
