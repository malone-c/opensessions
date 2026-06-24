use std::collections::BTreeMap;
use opensessions_zellij_core::{build_rows, PaneSnapshot, SessionSnapshot};
use zellij_tile::prelude::*;

#[derive(Default)]
struct State {
    sessions: Vec<SessionSnapshot>,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        request_permission(&[PermissionType::ReadApplicationState]);
        subscribe(&[EventType::SessionUpdate]);
    }

    fn update(&mut self, event: Event) -> bool {
        let Event::SessionUpdate(sessions, _resurrectable) = event else {
            return false;
        };
        self.sessions = to_snapshots(sessions);
        true
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        println!("\u{1b}[1mopensessions\u{1b}[0m\n");
        for row in build_rows(&self.sessions, usize::MAX) {
            let marker = if row.is_current { "\u{1b}[1m\u{25b8}" } else { " " };
            let agents = if row.agents.is_empty() {
                String::new()
            } else {
                format!("  \u{1b}[2m{}\u{1b}[0m", row.agents.join(", "))
            };
            println!("{marker} {}\u{1b}[0m  {}p{agents}", row.name, row.pane_count);
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
