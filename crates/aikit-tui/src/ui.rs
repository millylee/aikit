use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

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
        Paragraph::new("").block(Block::default().borders(Borders::ALL).title("Providers"));
    let details = Paragraph::new("").block(Block::default().borders(Borders::ALL).title("Details"));
    let targets = Paragraph::new("").block(Block::default().borders(Borders::ALL).title("Targets"));

    frame.render_widget(providers, panes_layout[0]);
    frame.render_widget(details, panes_layout[1]);
    frame.render_widget(targets, panes_layout[2]);

    let status = Paragraph::new(state.status.as_str());
    frame.render_widget(status, main_layout[1]);
}
