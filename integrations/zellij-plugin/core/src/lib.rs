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

const AGENT_ALIASES: &[(&str, &[&str])] = &[
    ("amp", &["amp", "amp-local"]),
    ("claude-code", &["claude", "claude-code"]),
    ("codex", &["codex"]),
    ("gemini", &["gemini"]),
    ("cursor", &["cursor", "cursor-agent"]),
    ("antigravity", &["agy", "antigravity"]),
    ("cline", &["cline"]),
    ("opencode", &["opencode", "open-code"]),
    ("github-copilot", &["copilot", "github-copilot", "ghcs"]),
    ("kimi", &["kimi", "kimi-code"]),
    ("kiro", &["kiro", "kiro-cli"]),
    ("droid", &["droid"]),
    ("grok", &["grok", "grok-build"]),
    ("pi", &["pi"]),
];

/// Match a pane's title or command against known agent CLI names.
pub fn detect_agent(pane: &PaneSnapshot) -> Option<String> {
    let command = pane.command.clone().unwrap_or_default().to_lowercase();
    let haystack = format!("{} {}", pane.title.to_lowercase(), command);
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
}
