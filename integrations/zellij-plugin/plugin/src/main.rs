use std::collections::BTreeMap;
use opensessions_zellij_core::{build_rows, move_selection, PaneSnapshot, SessionSnapshot};
use zellij_tile::prelude::*;

#[derive(Default)]
struct State {
    sessions: Vec<SessionSnapshot>,
    selected: usize,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);
        subscribe(&[EventType::SessionUpdate, EventType::Key]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::SessionUpdate(sessions, _resurrectable) => {
                self.sessions = to_snapshots(sessions);
                self.selected = self.selected.min(self.sessions.len().saturating_sub(1));
                true
            }
            Event::Key(key) => self.handle_key(key),
            _ => false,
        }
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        println!("\u{1b}[1mopensessions\u{1b}[0m\n");
        for row in build_rows(&self.sessions, self.selected) {
            let marker = if row.is_current { "\u{25b8}" } else { " " };
            let agents = if row.agents.is_empty() {
                String::new()
            } else {
                format!("  \u{1b}[2m{}\u{1b}[0m", row.agents.join(", "))
            };
            let line = format!("{marker} {}  {}p{agents}", row.name, row.pane_count);
            if row.selected {
                println!("\u{1b}[7m{line}\u{1b}[0m");
            } else {
                println!("{line}");
            }
        }
    }
}

impl State {
    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        match key.bare_key {
            BareKey::Char('j') | BareKey::Down => {
                self.selected = move_selection(self.selected, self.sessions.len(), 1);
                true
            }
            BareKey::Char('k') | BareKey::Up => {
                self.selected = move_selection(self.selected, self.sessions.len(), -1);
                true
            }
            BareKey::Enter => {
                if let Some(session) = self.sessions.get(self.selected) {
                    switch_session_with_focus(&session.name, None, None);
                }
                false
            }
            BareKey::Char('x') => {
                if let Some(session) = self.sessions.get(self.selected) {
                    if !session.is_current {
                        let _ = kill_sessions(&[session.name.clone()]);
                    }
                }
                false
            }
            _ => false,
        }
    }
}

/// Map Zellij's per-session PaneManifest into core snapshots, sorted by name so
/// the render order is stable across updates.
fn to_snapshots(sessions: Vec<SessionInfo>) -> Vec<SessionSnapshot> {
    let mut snapshots: Vec<SessionSnapshot> = sessions
        .into_iter()
        .map(|session| SessionSnapshot {
            name: session.name,
            is_current: session.is_current_session,
            panes: session
                .panes
                .panes
                .values()
                .flatten()
                .map(|pane| PaneSnapshot {
                    title: pane.title.clone(),
                    command: pane.terminal_command.clone(),
                    is_plugin: pane.is_plugin,
                })
                .collect(),
        })
        .collect();
    snapshots.sort_by(|a, b| a.name.cmp(&b.name));
    snapshots
}
