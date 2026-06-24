//! opensessions zellij plugin — Claude Code agent dashboard.
//!
//! Lists agent panes in the current session with live lifecycle status polled
//! from opensessions-server's `GET /agents` (joined to panes on cwd), and jumps
//! focus to a pane on Enter. Scoped to the current session: zellij's
//! SessionUpdate only surfaces the current session's panes to a plugin.

use std::collections::BTreeMap;

use opensessions_zellij_core::{
    build_agent_rows, move_selection, AgentStatus, AgentStatusEntry, DashboardPane,
};
use serde::Deserialize;
use zellij_tile::prelude::*;

const POLL_SECS: f64 = 1.5;

#[derive(Default)]
struct State {
    server_url: String,
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
                self.fetch_agents();
                set_timeout(POLL_SECS);
                false
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
        let rows = build_agent_rows(&self.panes, &self.statuses, self.selected);
        let note = if self.server_ok {
            String::new()
        } else {
            "  \u{1b}[2m(server: connecting…)\u{1b}[0m".to_string()
        };
        println!("\u{1b}[1mclaude agents\u{1b}[0m{note}\n");
        if rows.is_empty() {
            println!("\u{1b}[2mno agents in this session\u{1b}[0m");
            return;
        }
        for row in &rows {
            let (color, glyph) = status_style(row.status);
            let dir = row.cwd.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or(&row.cwd);
            let thread = row
                .thread_name
                .as_deref()
                .map(|name| format!("  \u{1b}[2m{name}\u{1b}[0m"))
                .unwrap_or_default();
            let line = format!(
                "{color}{glyph}\u{1b}[0m {} \u{1b}[2m{}\u{1b}[0m  {dir}{thread}",
                row.agent,
                row.status.label(),
            );
            if row.selected {
                println!("\u{1b}[7m{line}\u{1b}[0m");
            } else {
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

    /// Collect the current session's terminal panes, resolving each pane's cwd.
    fn rebuild_panes(&mut self, sessions: Vec<SessionInfo>) {
        self.panes.clear();
        let Some(current) = sessions.into_iter().find(|session| session.is_current_session) else {
            return;
        };
        for pane in current.panes.panes.values().flatten() {
            if pane.is_plugin {
                continue;
            }
            let cwd = get_pane_cwd(PaneId::Terminal(pane.id))
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.panes.push(DashboardPane {
                pane_id: pane.id,
                title: pane.title.clone(),
                command: pane.terminal_command.clone(),
                cwd,
            });
        }
        self.panes.sort_by_key(|pane| pane.pane_id);
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        let row_count = build_agent_rows(&self.panes, &self.statuses, self.selected).len();
        match key.bare_key {
            BareKey::Char('j') | BareKey::Down => {
                self.selected = move_selection(self.selected, row_count, 1);
                true
            }
            BareKey::Char('k') | BareKey::Up => {
                self.selected = move_selection(self.selected, row_count, -1);
                true
            }
            BareKey::Enter => {
                let rows = build_agent_rows(&self.panes, &self.statuses, self.selected);
                if let Some(row) = rows.get(self.selected) {
                    focus_pane_with_id(PaneId::Terminal(row.pane_id), false, false);
                }
                false
            }
            _ => false,
        }
    }

    fn clamp_selection(&mut self) {
        let row_count = build_agent_rows(&self.panes, &self.statuses, self.selected).len();
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
        })
        .collect()
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
