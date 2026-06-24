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

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(title: &str, command: Option<&str>) -> PaneSnapshot {
        PaneSnapshot { title: title.into(), command: command.map(str::to_string), is_plugin: false }
    }

    #[test]
    fn detects_agent_from_title_or_command() {
        assert_eq!(detect_agent(&pane("claude", None)).as_deref(), Some("claude-code"));
        assert_eq!(detect_agent(&pane("zsh", Some("codex"))).as_deref(), Some("codex"));
        assert_eq!(detect_agent(&pane("vim", Some("vim"))), None);
    }
}
