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

// Snapshot of the alias table in packages/runtime-rs/src/tmux_provider.rs
const AGENT_ALIASES: &[(&str, &[&str])] = &[
    ("amp", &["amp", "amp-local"]),
    ("claude-code", &["claude", "claude-code"]),
    ("codex", &["codex"]),
    ("gemini", &["gemini"]),
    ("cursor", &["cursor", "cursor-agent"]),
    ("antigravity", &["agy", "antigravity", "antigravity-cli"]),
    ("cline", &["cline"]),
    ("opencode", &["opencode", "open-code"]),
    ("github-copilot", &["copilot", "github-copilot", "ghcs"]),
    ("kimi", &["kimi", "kimi-code"]),
    ("kiro", &["kiro", "kiro-cli"]),
    ("droid", &["droid"]),
    ("grok", &["grok", "grok-build"]),
    ("hermes", &["hermes", "hermes-agent"]),
    ("qodercli", &["qodercli", "qoderclicn", "qoder", "qodercn"]),
];

/// Match a pane's title or command against known agent CLI names.
pub fn detect_agent(pane: &PaneSnapshot) -> Option<String> {
    let title = pane.title.to_lowercase();
    let command = pane.command.clone().unwrap_or_default().to_lowercase();
    if title == "pi" || title.starts_with("pi ") || title.starts_with('π') || command == "pi" {
        return Some("pi".to_string());
    }
    let haystack = format!("{} {}", title, command);
    AGENT_ALIASES
        .iter()
        .find(|(_, aliases)| aliases.iter().any(|alias| haystack.contains(alias)))
        .map(|(agent, _)| agent.to_string())
}

pub fn build_rows(sessions: &[SessionSnapshot], selected_index: usize) -> Vec<SidebarRow> {
    sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let mut agents: Vec<String> = session
                .panes
                .iter()
                .filter_map(detect_agent)
                .collect();
            agents.sort();
            agents.dedup();
            SidebarRow {
                name: session.name.clone(),
                is_current: session.is_current,
                pane_count: session.panes.iter().filter(|pane| !pane.is_plugin).count(),
                agents,
                selected: index == selected_index,
            }
        })
        .collect()
}

pub fn move_selection(selected: usize, len: usize, delta: i32) -> usize {
    if len == 0 {
        return 0;
    }
    let last = len - 1;
    let next = selected as i32 + delta;
    next.clamp(0, last as i32) as usize
}

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

pub struct DashboardPane {
    pub pane_id: u32,
    pub title: String,
    pub command: Option<String>,
    pub cwd: String,
}

pub struct AgentStatusEntry {
    pub cwd: String,
    pub agent: String,
    pub status: AgentStatus,
    pub thread_name: Option<String>,
}

pub struct AgentRow {
    pub pane_id: u32,
    pub agent: String,
    pub status: AgentStatus,
    pub cwd: String,
    pub thread_name: Option<String>,
    pub selected: bool,
}

/// One row per pane that is an agent: either detected by title/command, or whose
/// cwd matches a server-reported agent. Status comes from the server match, or
/// Idle when the pane looks like an agent but the server has no live status.
pub fn build_agent_rows(
    panes: &[DashboardPane],
    statuses: &[AgentStatusEntry],
    selected: usize,
) -> Vec<AgentRow> {
    let mut rows = Vec::new();
    for pane in panes {
        let canon = canonical_cwd(&pane.cwd);
        let matched = statuses
            .iter()
            .find(|status| canonical_cwd(&status.cwd) == canon);
        let detected = detect_agent(&PaneSnapshot {
            title: pane.title.clone(),
            command: pane.command.clone(),
            is_plugin: false,
        });
        if matched.is_none() && detected.is_none() {
            continue;
        }
        rows.push(AgentRow {
            pane_id: pane.pane_id,
            agent: matched
                .map(|status| status.agent.clone())
                .or(detected)
                .unwrap_or_else(|| "agent".to_string()),
            status: matched.map(|status| status.status).unwrap_or(AgentStatus::Idle),
            cwd: pane.cwd.clone(),
            thread_name: matched.and_then(|status| status.thread_name.clone()),
            selected: false,
        });
    }
    for (index, row) in rows.iter_mut().enumerate() {
        row.selected = index == selected;
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(title: &str, command: Option<&str>) -> PaneSnapshot {
        PaneSnapshot { title: title.into(), command: command.map(str::to_string), is_plugin: false }
    }

    fn session(name: &str, is_current: bool, panes: Vec<PaneSnapshot>) -> SessionSnapshot {
        SessionSnapshot { name: name.into(), is_current, panes }
    }

    #[test]
    fn detects_agent_from_title_or_command() {
        assert_eq!(detect_agent(&pane("claude", None)).as_deref(), Some("claude-code"));
        assert_eq!(detect_agent(&pane("zsh", Some("codex"))).as_deref(), Some("codex"));
        assert_eq!(detect_agent(&pane("vim", Some("vim"))), None);
    }

    #[test]
    fn builds_rows_with_counts_agents_and_selection() {
        let sessions = vec![
            session("a", true, vec![pane("claude", None), pane("zsh", None)]),
            session("b", false, vec![pane("plugin", None)]),
        ];
        let rows = build_rows(&sessions, 1);

        assert_eq!(rows[0].name, "a");
        assert_eq!(rows[0].pane_count, 2);
        assert_eq!(rows[0].agents, vec!["claude-code".to_string()]);
        assert!(rows[0].is_current);
        assert!(!rows[0].selected);

        assert_eq!(rows[1].name, "b");
        assert_eq!(rows[1].pane_count, 1);
        assert!(rows[1].agents.is_empty());
        assert!(rows[1].selected);
    }

    #[test]
    fn pi_agent_avoids_substring_false_positives() {
        assert_eq!(detect_agent(&pane("pipenv", None)), None);
        assert_eq!(detect_agent(&pane("pip", Some("pip"))), None);
        assert_eq!(detect_agent(&pane("pi", None)).as_deref(), Some("pi"));
        assert_eq!(detect_agent(&pane("pi chat", None)).as_deref(), Some("pi"));
        assert_eq!(detect_agent(&pane("zsh", Some("pi"))).as_deref(), Some("pi"));
    }

    #[test]
    fn move_selection_clamps_to_bounds() {
        assert_eq!(move_selection(0, 3, -1), 0);
        assert_eq!(move_selection(0, 3, 1), 1);
        assert_eq!(move_selection(2, 3, 1), 2);
        assert_eq!(move_selection(1, 0, 1), 0);
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

    fn dash_pane(pane_id: u32, title: &str, command: Option<&str>, cwd: &str) -> DashboardPane {
        DashboardPane {
            pane_id,
            title: title.into(),
            command: command.map(str::to_string),
            cwd: cwd.into(),
        }
    }

    fn status_entry(cwd: &str, agent: &str, status: AgentStatus) -> AgentStatusEntry {
        AgentStatusEntry { cwd: cwd.into(), agent: agent.into(), status, thread_name: None }
    }

    #[test]
    fn build_agent_rows_joins_on_cwd_and_handles_idle_and_filtering() {
        let panes = vec![
            // matched via server status, even though it's a bare shell (cwd differs by /private)
            dash_pane(1, "zsh", Some("zsh"), "/private/tmp/proj"),
            // detected as claude by command, but no server status -> idle
            dash_pane(2, "claude", Some("claude"), "/Users/cm/other"),
            // not an agent and no match -> excluded
            dash_pane(3, "vim", Some("vim"), "/Users/cm/misc"),
        ];
        let statuses = vec![status_entry("/tmp/proj", "claude-code", AgentStatus::ToolRunning)];

        let rows = build_agent_rows(&panes, &statuses, 1);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].pane_id, 1);
        assert_eq!(rows[0].agent, "claude-code");
        assert_eq!(rows[0].status, AgentStatus::ToolRunning);
        assert!(!rows[0].selected);

        assert_eq!(rows[1].pane_id, 2);
        assert_eq!(rows[1].status, AgentStatus::Idle);
        assert!(rows[1].selected);
    }
}
