use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
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
    let details = Paragraph::new(details_text(state)).block(pane_block(
        "Details",
        state.focused_pane == FocusedPane::Details,
    ));
    let targets = Paragraph::new(targets_text(state)).block(pane_block(
        "Targets",
        state.focused_pane == FocusedPane::Targets,
    ));

    frame.render_widget(providers, panes_layout[0]);
    frame.render_widget(details, panes_layout[1]);
    frame.render_widget(targets, panes_layout[2]);

    let status = Paragraph::new(state.status.as_str());
    frame.render_widget(status, main_layout[1]);

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
        format!("> {title} ACTIVE")
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
            let enabled = if provider.enabled {
                "enabled"
            } else {
                "disabled"
            };
            let model_count = provider
                .models_cache
                .as_ref()
                .map(|cache| cache.models.len())
                .unwrap_or(0)
                + provider.manual_models.len();
            format!(
                "{cursor}{active} {} ({enabled})\n  {} model(s)",
                provider.name, model_count
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn details_text(state: &AppState) -> String {
    let Some(provider) = state.selected_provider() else {
        return "Select a provider to view keys and cached models.".into();
    };
    let active = state.config.active_selection.as_ref();
    let mut lines = vec![
        format!("Provider: {}", provider.name),
        format!("Base URL: {}", provider.base_url),
        "Keys:".into(),
    ];

    if provider.api_keys.is_empty() {
        lines.push("  No API keys configured".into());
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
            lines.push(format!("{cursor}{active_key} {}", key.name));
        }
    }

    lines.push("Models:".into());
    let key_count = provider.api_keys.len();
    let cached_models = provider
        .models_cache
        .as_ref()
        .map(|cache| cache.models.as_slice())
        .unwrap_or(&[]);
    if cached_models.is_empty() && provider.manual_models.is_empty() {
        lines.push("  No models; press r to refresh or m to add one.".into());
    } else {
        for (index, model) in cached_models.iter().enumerate() {
            let detail_index = key_count + index;
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
            let detail_index = key_count + cached_models.len() + index;
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
            lines.push(format!("{cursor}{active_model} {model} [manual]"));
        }
        if let Some(cache) = provider.models_cache.as_ref() {
            lines.push(format!("Cache refreshed: {}", cache.refreshed_at));
            if let Some(error) = &cache.last_error {
                lines.push(format!("Last refresh error: {error}"));
            }
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
            let path = target
                .config_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "default path".into());
            let status = state.target_status(&target.id).unwrap_or("not applied");
            format!("{cursor} {enabled} {}\n  {path}\n  {status}", target.id)
        })
        .collect::<Vec<_>>()
        .join("\n")
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
                    "{} id: {}",
                    field_cursor(form.current_field == 0),
                    form.id.as_str()
                ),
                format!(
                    "{} name: {}",
                    field_cursor(form.current_field == 1),
                    form.name.as_str()
                ),
                format!(
                    "{} base_url: {}",
                    field_cursor(form.current_field == 2),
                    form.base_url.as_str()
                ),
                format!(
                    "{} enabled: {}",
                    field_cursor(form.current_field == 3),
                    form.enabled.as_str()
                ),
                String::new(),
                "Tab/Shift+Tab switch field, Enter save, Esc cancel".into(),
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
            let value = form.value.clone();
            let mut lines = vec![
                format!("{} value: {}", field_cursor(true), value),
                String::new(),
                "Enter save, Esc cancel".into(),
            ];
            if let Some(error) = &form.validation_error {
                lines.push(format!("Error: {error}"));
            }
            render_modal_text(frame, area, title, lines.join("\n"));
        }
        ModalState::ModelForm(form) => {
            let mut lines = vec![
                format!("{} model: {}", field_cursor(true), form.model.as_str()),
                String::new(),
                "Enter save, Esc cancel".into(),
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
    }
}

fn render_modal_text(frame: &mut Frame, area: Rect, title: &str, text: String) {
    frame.render_widget(Clear, area);
    let widget = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(title))
        .alignment(Alignment::Left);
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
