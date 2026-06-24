//! Pure dashboard logic for the opensessions zellij Claude-agent dashboard.
//! Dependency-free so it can be unit-tested on the host (the plugin crate is
//! wasm-only and can't be).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AgentStatus {
    Idle,
    Running,
    ToolRunning,
    Waiting,
    Done,
    Error,
    Interrupted,
    Stale,
    Unknown,
}

impl AgentStatus {
    pub fn parse(value: &str) -> AgentStatus {
        match value {
            "idle" => Self::Idle,
            "running" => Self::Running,
            "tool-running" => Self::ToolRunning,
            "waiting" => Self::Waiting,
            "done" => Self::Done,
            "error" => Self::Error,
            "interrupted" => Self::Interrupted,
            "stale" => Self::Stale,
            _ => Self::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::ToolRunning => "tool-running",
            Self::Waiting => "waiting",
            Self::Done => "done",
            Self::Error => "error",
            Self::Interrupted => "interrupted",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }

    pub fn glyph(&self) -> &'static str {
        match self {
            Self::Running | Self::ToolRunning => "●",
            Self::Waiting => "◆",
            Self::Done => "✓",
            Self::Error => "✗",
            Self::Idle | Self::Stale | Self::Interrupted | Self::Unknown => "·",
        }
    }
}

/// Normalize a path for cross-source comparison: drop a trailing slash and the
/// macOS `/private` prefix (so `/private/tmp` and `/tmp` compare equal).
pub fn canonical_cwd(path: &str) -> String {
    let trimmed = path.strip_suffix('/').unwrap_or(path);
    match trimmed.strip_prefix("/private/") {
        Some(rest) => format!("/{rest}"),
        None => trimmed.to_string(),
    }
}

pub fn move_selection(selected: usize, len: usize, delta: i32) -> usize {
    if len == 0 {
        return 0;
    }
    let last = len - 1;
    let next = selected as i32 + delta;
    next.clamp(0, last as i32) as usize
}

/// A terminal pane in the current zellij session, with its resolved cwd.
pub struct DashboardPane {
    pub pane_id: u32,
    pub cwd: String,
}

/// An agent reported by the server's `GET /agents`, keyed by cwd.
pub struct AgentStatusEntry {
    pub cwd: String,
    pub agent: String,
    pub status: AgentStatus,
    pub thread_name: Option<String>,
}

pub struct AgentRow {
    pub agent: String,
    pub status: AgentStatus,
    pub cwd: String,
    pub thread_name: Option<String>,
    /// Pane to focus on Enter, if this agent has a pane in the current session.
    pub pane_id: Option<u32>,
    pub selected: bool,
}

/// One row per server-reported agent (so every running agent shows, regardless
/// of which zellij session holds its pane), resolving a current-session pane to
/// jump to when one matches on cwd. Sorted by cwd for stable ordering.
pub fn build_agent_rows(
    statuses: &[AgentStatusEntry],
    panes: &[DashboardPane],
    selected: usize,
) -> Vec<AgentRow> {
    let mut rows: Vec<AgentRow> = statuses
        .iter()
        .map(|status| {
            let canon = canonical_cwd(&status.cwd);
            let pane_id = panes
                .iter()
                .find(|pane| canonical_cwd(&pane.cwd) == canon)
                .map(|pane| pane.pane_id);
            AgentRow {
                agent: status.agent.clone(),
                status: status.status,
                cwd: status.cwd.clone(),
                thread_name: status.thread_name.clone(),
                pane_id,
                selected: false,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.cwd.cmp(&b.cwd));
    for (index, row) in rows.iter_mut().enumerate() {
        row.selected = index == selected;
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(cwd: &str, status: AgentStatus) -> AgentStatusEntry {
        AgentStatusEntry {
            cwd: cwd.into(),
            agent: "claude-code".into(),
            status,
            thread_name: None,
        }
    }

    #[test]
    fn agent_status_parses_kebab_case_and_unknown() {
        assert_eq!(AgentStatus::parse("tool-running"), AgentStatus::ToolRunning);
        assert_eq!(AgentStatus::parse("waiting"), AgentStatus::Waiting);
        assert_eq!(AgentStatus::parse("nonsense"), AgentStatus::Unknown);
    }

    #[test]
    fn canonical_cwd_strips_private_prefix_and_trailing_slash() {
        assert_eq!(canonical_cwd("/private/tmp"), "/tmp");
        assert_eq!(canonical_cwd("/Users/cm/proj/"), "/Users/cm/proj");
        assert_eq!(canonical_cwd("/Users/cm/proj"), "/Users/cm/proj");
    }

    #[test]
    fn move_selection_clamps_to_bounds() {
        assert_eq!(move_selection(0, 3, -1), 0);
        assert_eq!(move_selection(0, 3, 1), 1);
        assert_eq!(move_selection(2, 3, 1), 2);
        assert_eq!(move_selection(1, 0, 1), 0);
    }

    #[test]
    fn build_agent_rows_shows_every_agent_and_resolves_jump_pane() {
        let statuses = vec![
            entry("/Users/cm/b", AgentStatus::ToolRunning),
            entry("/Users/cm/a", AgentStatus::Idle),
        ];
        // A current-session pane matches "a" (via /private canonicalization); "b"
        // has no current-session pane, so it shows but isn't jumpable.
        let panes = vec![DashboardPane { pane_id: 7, cwd: "/private/Users/cm/a".into() }];

        let rows = build_agent_rows(&statuses, &panes, 0);

        assert_eq!(rows.len(), 2, "every agent shows regardless of pane presence");
        // sorted by cwd: "a" before "b"
        assert_eq!(rows[0].cwd, "/Users/cm/a");
        assert_eq!(rows[0].pane_id, Some(7));
        assert!(rows[0].selected);
        assert_eq!(rows[1].cwd, "/Users/cm/b");
        assert_eq!(rows[1].status, AgentStatus::ToolRunning);
        assert_eq!(rows[1].pane_id, None);
    }
}
