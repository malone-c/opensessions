pub struct PaneSnapshot {
    pub title: String,
    pub command: Option<String>,
    pub is_plugin: bool,
}

pub struct SessionSnapshot {
    pub name: String,
    pub is_current: bool,
    pub panes: Vec<PaneSnapshot>,
}

pub struct SidebarRow {
    pub name: String,
    pub is_current: bool,
    pub pane_count: usize,
    pub agents: Vec<String>,
    pub selected: bool,
}
