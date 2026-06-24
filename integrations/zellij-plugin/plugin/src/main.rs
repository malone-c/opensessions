//! opensessions zellij plugin — Claude Code agent dashboard.
//!
//! Renders one row per agent reported by opensessions-server's `GET /agents`
//! (so every running agent shows, regardless of which zellij session holds it),
//! with live lifecycle status. Enter jumps to the agent's pane when that pane is
//! in the current session (the only panes a zellij plugin can focus).

use std::collections::BTreeMap;

use opensessions_zellij_core::{
    build_agent_rows, move_selection, AgentRow, AgentStatus, AgentStatusEntry, DashboardPane,
};
use serde::Deserialize;
use zellij_tile::prelude::*;

const POLL_SECS: f64 = 1.5;

#[derive(Default)]
struct State {
    server_url: String,
    pane_ids: Vec<u32>,
    panes: Vec<DashboardPane>,
    statuses: Vec<AgentStatusEntry>,
    selected: usize,
    server_ok: bool,
}

#[derive(Deserialize)]
struct AgentDto {
    cwd: String,
    agent: String,
    status: String,
    #[serde(default)]
    thread_name: Option<String>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    worktree: Option<String>,
    #[serde(default)]
    folder: Option<String>,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.server_url = configuration
            .get("server_url")
            .cloned()
            .unwrap_or_else(|| "http://127.0.0.1:7391".to_string());
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::WebAccess,
        ]);
        subscribe(&[
            EventType::SessionUpdate,
            EventType::Key,
            EventType::Timer,
            EventType::WebRequestResult,
        ]);
        self.fetch_agents();
        set_timeout(POLL_SECS);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::SessionUpdate(sessions, _resurrectable) => {
                self.rebuild_panes(sessions);
                self.clamp_selection();
                true
            }
            Event::Timer(_) => {
                // Re-resolve pane cwds each tick: get_pane_cwd can return empty
                // for a freshly-started pane, so retry until it's available.
                self.resolve_pane_cwds();
                self.fetch_agents();
                set_timeout(POLL_SECS);
                true
            }
            Event::WebRequestResult(status, _headers, body, context) => {
                if context.get("req").map(String::as_str) != Some("agents") {
                    return false;
                }
                self.server_ok = status == 200;
                if self.server_ok {
                    self.statuses = parse_agents(&body);
                    self.clamp_selection();
                }
                true
            }
            Event::Key(key) => self.handle_key(key),
            _ => false,
        }
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        let rows = build_agent_rows(&self.statuses, &self.panes, self.selected);
        let note = if self.server_ok {
            String::new()
        } else {
            "  \u{1b}[2m(server: connecting…)\u{1b}[0m".to_string()
        };
        println!("\u{1b}[1mclaude agents ({})\u{1b}[0m{note}\n", rows.len());
        if rows.is_empty() {
            println!("\u{1b}[2mno agents\u{1b}[0m");
            return;
        }
        for row in &rows {
            for line in agent_lines(row) {
                println!("{line}");
            }
        }
    }
}

impl State {
    fn fetch_agents(&self) {
        let mut context = BTreeMap::new();
        context.insert("req".to_string(), "agents".to_string());
        web_request(
            format!("{}/agents", self.server_url),
            HttpVerb::Get,
            BTreeMap::new(),
            Vec::new(),
            context,
        );
    }

    /// Record the current session's terminal pane ids, then resolve their cwds.
    fn rebuild_panes(&mut self, sessions: Vec<SessionInfo>) {
        self.pane_ids.clear();
        if let Some(current) = sessions.into_iter().find(|session| session.is_current_session) {
            for pane in current.panes.panes.values().flatten() {
                if !pane.is_plugin {
                    self.pane_ids.push(pane.id);
                }
            }
        }
        self.resolve_pane_cwds();
    }

    /// Resolve each known pane id to its cwd (so agent rows can find a pane to
    /// jump to). Called on SessionUpdate and on each timer tick, since
    /// get_pane_cwd lags pane creation.
    fn resolve_pane_cwds(&mut self) {
        self.panes = self
            .pane_ids
            .iter()
            .filter_map(|&pane_id| {
                let cwd = get_pane_cwd(PaneId::Terminal(pane_id))
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (!cwd.is_empty()).then_some(DashboardPane { pane_id, cwd })
            })
            .collect();
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        let rows = build_agent_rows(&self.statuses, &self.panes, self.selected);
        match key.bare_key {
            BareKey::Char('j') | BareKey::Down => {
                self.selected = move_selection(self.selected, rows.len(), 1);
                true
            }
            BareKey::Char('k') | BareKey::Up => {
                self.selected = move_selection(self.selected, rows.len(), -1);
                true
            }
            BareKey::Enter => {
                if let Some(pane_id) = rows.get(self.selected).and_then(|row| row.pane_id) {
                    focus_pane_with_id(PaneId::Terminal(pane_id), false, false);
                }
                false
            }
            _ => false,
        }
    }

    fn clamp_selection(&mut self) {
        let row_count = build_agent_rows(&self.statuses, &self.panes, self.selected).len();
        self.selected = self.selected.min(row_count.saturating_sub(1));
    }
}

fn parse_agents(body: &[u8]) -> Vec<AgentStatusEntry> {
    serde_json::from_slice::<Vec<AgentDto>>(body)
        .unwrap_or_default()
        .into_iter()
        .map(|dto| AgentStatusEntry {
            cwd: dto.cwd,
            agent: dto.agent,
            status: AgentStatus::parse(&dto.status),
            thread_name: dto.thread_name,
            repo: dto.repo,
            branch: dto.branch,
            worktree: dto.worktree,
            folder: dto.folder,
        })
        .collect()
}

/// One or two display lines for an agent: line 1 is status + repo + branch
/// (or the folder name when not in a repo); line 2 (when present) is where the
/// agent is open — worktree/subfolder — plus its thread name.
fn agent_lines(row: &AgentRow) -> Vec<String> {
    let (color, glyph) = status_style(row.status);
    let marker = if row.selected { "\u{1b}[1m\u{276f}\u{1b}[0m" } else { " " };
    let jump = if row.pane_id.is_some() { "\u{1b}[2m↵\u{1b}[0m" } else { " " };
    let bold = if row.selected { "\u{1b}[1m" } else { "" };

    let identity = match &row.repo {
        Some(repo) => {
            let branch = row
                .branch
                .as_deref()
                .map(|branch| format!("  \u{1b}[2m{branch}\u{1b}[0m"))
                .unwrap_or_default();
            format!("{bold}{repo}\u{1b}[0m{branch}")
        }
        None => {
            let dir = row.cwd.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or(&row.cwd);
            format!("{bold}{dir}\u{1b}[0m")
        }
    };
    let line1 = format!(
        "{marker}{jump} {color}{glyph}\u{1b}[0m \u{1b}[2m{}\u{1b}[0m  {identity}",
        row.status.label(),
    );
    let mut lines = vec![line1];

    let mut location = Vec::new();
    if let Some(worktree) = &row.worktree {
        location.push(worktree.as_str());
    }
    if let Some(folder) = &row.folder {
        location.push(folder.as_str());
    }
    let location = location.join("/");
    let thread = row.thread_name.as_deref().filter(|name| !name.is_empty());
    if !location.is_empty() || thread.is_some() {
        let mut detail = format!("     \u{1b}[2m{location}");
        if let Some(thread) = thread {
            if !location.is_empty() {
                detail.push_str("  ·  ");
            }
            detail.push_str(thread);
        }
        detail.push_str("\u{1b}[0m");
        lines.push(detail);
    }
    lines
}

fn status_style(status: AgentStatus) -> (&'static str, &'static str) {
    let color = match status {
        AgentStatus::Running | AgentStatus::ToolRunning => "\u{1b}[33m",
        AgentStatus::Waiting => "\u{1b}[35m",
        AgentStatus::Done => "\u{1b}[32m",
        AgentStatus::Error => "\u{1b}[31m",
        _ => "\u{1b}[2m",
    };
    (color, status.glyph())
}
