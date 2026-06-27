#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPane {
    Providers,
    Details,
    Targets,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    pub focused_pane: FocusedPane,
    pub status: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            focused_pane: FocusedPane::Providers,
            status: "Ready".into(),
        }
    }
}

impl AppState {
    pub fn focus_next_pane(&mut self) {
        self.focused_pane = match self.focused_pane {
            FocusedPane::Providers => FocusedPane::Details,
            FocusedPane::Details => FocusedPane::Targets,
            FocusedPane::Targets => FocusedPane::Providers,
        };
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }
}
