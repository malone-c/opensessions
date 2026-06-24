//! Throwaway spike: does a Zellij plugin see cwd and focus for panes that live
//! in OTHER sessions? Load from one session while Claude panes run in others.

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

#[derive(Default)]
struct State {
    rows: Vec<Row>,
    selected: usize,
}

struct Row {
    session: String,
    is_current_session: bool,
    pane_id: u32,
    title: String,
    cwd: String,
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
                self.rows.clear();
                for session in sessions {
                    for pane in session.panes.panes.values().flatten() {
                        if pane.is_plugin {
                            continue;
                        }
                        // get_pane_cwd is the thing under test, for panes in
                        // BOTH the current session and other sessions.
                        let cwd = match get_pane_cwd(PaneId::Terminal(pane.id)) {
                            Ok(path) => path.to_string_lossy().into_owned(),
                            Err(err) => format!("ERR: {err}"),
                        };
                        self.rows.push(Row {
                            session: session.name.clone(),
                            is_current_session: session.is_current_session,
                            pane_id: pane.id,
                            title: pane.title.clone(),
                            cwd,
                        });
                    }
                }
                self.rows.sort_by(|a, b| a.session.cmp(&b.session).then(a.pane_id.cmp(&b.pane_id)));
                self.selected = self.selected.min(self.rows.len().saturating_sub(1));
                true
            }
            Event::Key(key) => match key.bare_key {
                BareKey::Char('j') | BareKey::Down => {
                    if !self.rows.is_empty() {
                        self.selected = (self.selected + 1).min(self.rows.len() - 1);
                    }
                    true
                }
                BareKey::Char('k') | BareKey::Up => {
                    self.selected = self.selected.saturating_sub(1);
                    true
                }
                BareKey::Enter => {
                    if let Some(row) = self.rows.get(self.selected) {
                        focus_pane_with_id(PaneId::Terminal(row.pane_id), false, false);
                    }
                    false
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        println!("\u{1b}[1mcross-session spike\u{1b}[0m  (Enter = jump)\n");
        for (index, row) in self.rows.iter().enumerate() {
            let marker = if index == self.selected { "\u{1b}[7m>" } else { " " };
            let scope = if row.is_current_session { "[here]" } else { "[other]" };
            println!(
                "{marker} {scope} {}/%{}  {}  cwd={}\u{1b}[0m",
                row.session, row.pane_id, row.title, row.cwd
            );
        }
    }
}
