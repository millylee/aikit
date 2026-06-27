use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, FocusedPane};

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

    let providers =
        Paragraph::new(providers_text(state)).block(Block::default().borders(Borders::ALL).title(
            pane_title("Providers", state.focused_pane == FocusedPane::Providers),
        ));
    let details =
        Paragraph::new(details_text(state)).block(Block::default().borders(Borders::ALL).title(
            pane_title("Details", state.focused_pane == FocusedPane::Details),
        ));
    let targets =
        Paragraph::new(targets_text(state)).block(Block::default().borders(Borders::ALL).title(
            pane_title("Targets", state.focused_pane == FocusedPane::Targets),
        ));

    frame.render_widget(providers, panes_layout[0]);
    frame.render_widget(details, panes_layout[1]);
    frame.render_widget(targets, panes_layout[2]);

    let status = Paragraph::new(state.status.as_str());
    frame.render_widget(status, main_layout[1]);
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
            let enabled = if provider.enabled {
                "enabled"
            } else {
                "disabled"
            };
            let model_count = provider
                .models_cache
                .as_ref()
                .map(|cache| cache.models.len())
                .unwrap_or(0);
            format!(
                "{cursor}{active} {} ({enabled})\n  {} cached model(s)",
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
    match provider.models_cache.as_ref() {
        Some(cache) if !cache.models.is_empty() => {
            for (index, model) in cache.models.iter().enumerate() {
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
            lines.push(format!("Cache refreshed: {}", cache.refreshed_at));
            if let Some(error) = &cache.last_error {
                lines.push(format!("Last refresh error: {error}"));
            }
        }
        _ => lines.push("  No cached models; press r to refresh.".into()),
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
