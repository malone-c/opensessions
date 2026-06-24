# Claude Agent Dashboard — Part 1: Spike + Server Endpoint — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** De-risk the dashboard (verify cross-session pane cwd/focus in Zellij) and make `opensessions-server` expose live Claude agent status keyed by working directory under Zellij.

**Architecture:** Two independent pieces. (1) A throwaway Zellij plugin spike that proves a plugin can read `get_pane_cwd` and focus panes that live in *other* Zellij sessions — this gates the later plugin-dashboard plan. (2) Server changes so it runs headless under Zellij (watchers + HTTP, no mux provider) and serves `GET /agents` returning agent status keyed by cwd, sourced from the existing transcript watchers.

**Tech Stack:** Rust; `zellij-tile = "0.44"`; `wasm32-wasip1` (spike only); `tokio` HTTP server (`apps/server-rs`); `serde_json`.

**Reference spec:** `docs/superpowers/specs/2026-06-24-claude-agent-dashboard-design.md`

## Global Constraints

- Branch: `claude-agent-dashboard` (already created off `zellij-sidebar`). Stay on it.
- `zellij-tile` pinned to "0.44" (spike crate).
- Spike builds for `wasm32-wasip1`; server builds host target.
- Use the rustup `stable` toolchain (plain `cargo` resolves to it; `~/.zshrc` already sets PATH). If `cargo` is not found in a shell, prefix with `PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"`.
- The join key between agent status and a pane is the **cwd**. The server returns the agent's real absolute cwd (`project_dir`); canonicalization happens plugin-side (Part 2).
- Agents reported by `/agents` are non-idle only; an idle/unknown Claude pane is one with no `/agents` entry (handled plugin-side in Part 2).
- Comments sparse; no docstring type annotations; avoid single-call-site helpers; no leading-underscore names except genuinely-unused params.
- Commit messages one line, no mention of Claude/AI.

---

## File Structure

```
integrations/zellij-plugin/
  Cargo.toml                       # add "spike" to workspace members (temporary)
  spike/
    Cargo.toml                     # throwaway crate (deleted after the spike resolves)
    src/main.rs                    # cross-session cwd + focus probe

apps/server-rs/src/lib.rs          # headless source + agents_by_cwd field + apply-snapshot record
                                   #   + StateSource::agents_json + GET /agents branch
packages/runtime-rs/src/server_state.rs  # (read-only reference; no change expected)
```

---

## Phase 0: Cross-Session Spike (gates Part 2)

### Task 0.1: Throwaway plugin proving cross-session cwd + focus

**Files:**
- Modify: `integrations/zellij-plugin/Cargo.toml` (add `spike` member)
- Create: `integrations/zellij-plugin/spike/Cargo.toml`
- Create: `integrations/zellij-plugin/spike/src/main.rs`

**Interfaces:**
- Produces: nothing consumed by later tasks — this is a manual-verification artifact whose RESULT (does cross-session cwd/focus work?) determines Part 2's scope.

- [ ] **Step 1: Add the spike crate to the workspace**

Edit `integrations/zellij-plugin/Cargo.toml` members line to:
```toml
members = ["core", "plugin", "spike"]
```

- [ ] **Step 2: Create the spike manifest**

`integrations/zellij-plugin/spike/Cargo.toml`:
```toml
[package]
name = "opensessions-zellij-spike"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
zellij-tile = "0.44"
```

- [ ] **Step 3: Write the spike**

`integrations/zellij-plugin/spike/src/main.rs`:
```rust
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
                        switch_session_with_focus(&row.session, None, Some((row.pane_id, false)));
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
```

- [ ] **Step 4: Build the spike**

Run:
```bash
cd ~/dev/opensessions/integrations/zellij-plugin
cargo build -p opensessions-zellij-spike --target wasm32-wasip1 --release
```
Expected: clean build; `target/wasm32-wasip1/release/opensessions-zellij-spike.wasm` exists.

- [ ] **Step 5: Commit**

```bash
cd ~/dev/opensessions
git add integrations/zellij-plugin/Cargo.toml integrations/zellij-plugin/spike
git commit -m "add cross-session cwd/focus spike"
```

- [ ] **Step 6: Manual verification (human — record the result)**

This step is performed by the human, not a subagent. With at least two Zellij sessions running and a Claude/terminal pane in a session OTHER than the one you load from:
```bash
zellij action launch-or-focus-plugin --skip-plugin-cache \
  file:$HOME/dev/opensessions/integrations/zellij-plugin/target/wasm32-wasip1/release/opensessions-zellij-spike.wasm
```
Record two facts in the Part 2 plan's preamble:
1. Do `[other]`-session rows show a real `cwd=` (not `ERR:`)? → cross-session `get_pane_cwd` works.
2. Does Enter on an `[other]` row jump to that pane in the other session? → cross-session focus works.

If both YES → Part 2 covers all sessions. If NO → Part 2 scopes the dashboard to the current session and notes the limitation.

---

## Phase 1: Server — headless mode + `GET /agents`

### Task 1.1: Run the server headless under Zellij

**Files:**
- Modify: `apps/server-rs/src/lib.rs` — `default_state_source_from_env` (around line 369)

**Interfaces:**
- Consumes: `ReadOnlyMuxStateSource::new(vec![])` (existing constructor; empty providers).
- Produces: a state source under Zellij so `start_background_tasks` (the watcher loop) and HTTP serving run with no mux provider.

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)]` module in `apps/server-rs/src/lib.rs`:
```rust
#[test]
fn state_source_runs_headless_under_zellij() {
    let env = |key: &str| match key {
        "ZELLIJ_SESSION_NAME" => Some("main".to_string()),
        _ => None,
    };
    let source = default_state_source_from_env(env);
    assert!(source.is_some(), "expected a headless state source under zellij");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ~/dev/opensessions && cargo test -p opensessions-server state_source_runs_headless_under_zellij`
Expected: FAIL (returns `None` today).

- [ ] **Step 3: Implement the headless branch**

In `default_state_source_from_env`, after the existing `if env("TMUX").is_some() { ... }` block and before the final `None`, add:
```rust
    if env("ZELLIJ_SESSION_NAME").is_some() {
        // Headless agent-only mode: no mux provider, watchers still run and the
        // HTTP server still serves. Mux-dependent endpoints become no-ops.
        let mut source = ReadOnlyMuxStateSource::new(vec![]);
        let config = env("HOME")
            .map(PathBuf::from)
            .map(|home| load_config_from_home(&home));
        if let Some(width) = config.as_ref().and_then(|config| config.sidebar_width) {
            source = source.with_sidebar_width(clamp_sidebar_width(width) as u32);
        }
        if let Some(height) = config.and_then(|config| config.detail_panel_height) {
            source = source.with_detail_panel_height(height);
        }
        return Some(source);
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p opensessions-server state_source_runs_headless_under_zellij`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd ~/dev/opensessions
git add apps/server-rs/src/lib.rs
git commit -m "run server headless under zellij"
```

### Task 1.2: Retain agent status keyed by cwd

**Files:**
- Modify: `apps/server-rs/src/lib.rs` — `ReadOnlyMuxStateSource` struct (around line 343), `ReadOnlyMuxStateSource::new` (around line 391), `apply_agent_watcher_snapshot` (line 1163)

**Interfaces:**
- Produces: `agents_by_cwd: Mutex<HashMap<String, AgentByCwd>>` on `ReadOnlyMuxStateSource`, and a serializable `AgentByCwd { agent, status, cwd, thread_name, last_user_prompt, ts }` recorded for every non-idle watcher snapshot. Consumed by Task 1.3.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` module:
```rust
#[test]
fn records_agent_status_by_cwd_without_mux() {
    let source = ReadOnlyMuxStateSource::new(vec![]);
    let snapshot = AgentWatcherSnapshot {
        agent: "claude-code".to_string(),
        status: AgentStatus::Running,
        ts: 123,
        thread_id: Some("t1".to_string()),
        thread_name: Some("auth".to_string()),
        last_user_prompt: Some("fix login".to_string()),
        project_dir: Some("/Users/me/proj".to_string()),
    };
    source.apply_agent_watcher_snapshot(snapshot);

    let json = source.agents_json();
    assert!(json.contains("\"cwd\":\"/Users/me/proj\""), "got: {json}");
    assert!(json.contains("\"status\":\"running\""), "got: {json}");
    assert!(json.contains("\"agent\":\"claude-code\""), "got: {json}");
}
```
(If `AgentWatcherSnapshot` has fields beyond these, construct it via its existing constructor/`..Default::default()`; check its definition at the top of `packages/runtime-rs/src/agent_watchers.rs` and match it exactly.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p opensessions-server records_agent_status_by_cwd_without_mux`
Expected: FAIL (`agents_json` not defined; field missing).

- [ ] **Step 3: Add the record type and field**

Near the other types in `apps/server-rs/src/lib.rs`, add:
```rust
#[derive(Clone, serde::Serialize)]
struct AgentByCwd {
    agent: String,
    status: AgentStatus,
    cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_user_prompt: Option<String>,
    ts: u64,
}
```
Add the field to the `ReadOnlyMuxStateSource` struct (alongside `agent_tracker`):
```rust
    agents_by_cwd: Mutex<HashMap<String, AgentByCwd>>,
```
Initialize it in `ReadOnlyMuxStateSource::new` (alongside `agent_tracker: Mutex::new(AgentTracker::new()),`):
```rust
            agents_by_cwd: Mutex::new(HashMap::new()),
```

- [ ] **Step 4: Record in apply_agent_watcher_snapshot**

In `apply_agent_watcher_snapshot`, immediately AFTER the idle early-return (line ~1170, the `if snapshot.status == AgentStatus::Idle { ... return false; }` block) and BEFORE the `resolve_agent_watcher_session` line, insert:
```rust
        // Retain status keyed by the real cwd so the zellij dashboard can join
        // on it even when no mux provider attributes the agent to a session.
        if let Some(project_dir) = snapshot.project_dir.as_deref() {
            if !project_dir.starts_with("__encoded__:") {
                self.agents_by_cwd.lock().unwrap().insert(
                    project_dir.to_string(),
                    AgentByCwd {
                        agent: snapshot.agent.to_string(),
                        status: snapshot.status,
                        cwd: project_dir.to_string(),
                        thread_name: snapshot.thread_name.clone(),
                        last_user_prompt: snapshot.last_user_prompt.clone(),
                        ts: snapshot.ts,
                    },
                );
            }
        }
```

- [ ] **Step 5: Add agents_json**

Add an `agents_json` method to `impl ReadOnlyMuxStateSource` (a normal inherent method; Task 1.3 also exposes it on the trait). Prune entries older than 5 minutes so dead agents disappear:
```rust
    pub fn agents_json(&self) -> String {
        let now = (self.now_ms)();
        let cutoff = now.saturating_sub(5 * 60 * 1000);
        let agents: Vec<AgentByCwd> = self
            .agents_by_cwd
            .lock()
            .unwrap()
            .values()
            .filter(|entry| entry.ts >= cutoff)
            .cloned()
            .collect();
        serde_json::to_string(&agents).unwrap_or_else(|_| "[]".to_string())
    }
```
(`now_ms` timestamps are milliseconds — confirm against `current_time_ms`; if `ts` on snapshots is in seconds, compare in seconds instead.)

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p opensessions-server records_agent_status_by_cwd_without_mux`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cd ~/dev/opensessions
git add apps/server-rs/src/lib.rs
git commit -m "retain agent status keyed by cwd"
```

### Task 1.3: Serve `GET /agents`

**Files:**
- Modify: `apps/server-rs/src/lib.rs` — `StateSource` trait (add defaulted `agents_json`), `handle_connection` (add a branch near line 2504)

**Interfaces:**
- Consumes: `ReadOnlyMuxStateSource::agents_json` (Task 1.2).
- Produces: `GET /agents` → `200 OK` with a JSON array body; `[]` when no state source.

- [ ] **Step 1: Add agents_json to the StateSource trait**

Find the `trait StateSource` definition (it declares `snapshot_json`, `handle_http_text`, etc.). Add a defaulted method:
```rust
    fn agents_json(&self) -> String {
        "[]".to_string()
    }
```
And in `impl StateSource for ReadOnlyMuxStateSource`, add the override delegating to the inherent method:
```rust
    fn agents_json(&self) -> String {
        ReadOnlyMuxStateSource::agents_json(self)
    }
```

- [ ] **Step 2: Add the GET /agents branch**

In `handle_connection`, after the `/refresh` branch (line ~2513), add:
```rust
    if parsed.method == "GET" && parsed.path == "/agents" {
        let body = state_source
            .as_ref()
            .map(|state_source| state_source.agents_json())
            .unwrap_or_else(|| "[]".to_string());
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).await?;
        let _ = stream.shutdown().await;
        return Ok(());
    }
```

- [ ] **Step 3: Build to verify it compiles**

Run: `cd ~/dev/opensessions && cargo build -p opensessions-server`
Expected: builds clean.

- [ ] **Step 4: Integration check (manual, quick)**

Run the server headless and curl the endpoint:
```bash
ZELLIJ_SESSION_NAME=main ./target/debug/opensessions-server &
sleep 1
curl -s http://127.0.0.1:7391/agents; echo
kill %1
```
Expected: prints `[]` (no active Claude agents) or a JSON array if a Claude transcript is active. Confirms the route + headless run.

- [ ] **Step 5: Commit**

```bash
cd ~/dev/opensessions
git add apps/server-rs/src/lib.rs
git commit -m "serve GET /agents with cwd-keyed status"
```

### Task 1.4: Full server test run

**Files:** none (verification + ledger)

- [ ] **Step 1: Run the server test suite**

Run: `cd ~/dev/opensessions && cargo test -p opensessions-server 2>&1 | tail -20`
Expected: all tests pass (including the two new ones), output pristine.

- [ ] **Step 2: Commit any fixups** (only if needed; otherwise skip)

---

## What comes next (Part 2 — separate plan)

After the Phase 0 spike result is recorded, write **Part 2: Plugin Dashboard** — extend the `core` crate (`AgentStatus` parse, `canonical_cwd`, `join_agents`) and the `plugin` crate (enumerate Claude panes, `get_pane_cwd`, poll `GET /agents`, render full-lifecycle status, Enter→jump). Its pane-enumeration and jump scope (all sessions vs current session) is set by the spike outcome. Delete the `spike` crate (and its workspace member entry) as part of Part 2's first task.

---

## Self-Review

**Spec coverage:** Headless server (spec §Components/Server 1) → Task 1.1 ✓. Expose cwd-keyed status (§Server 2) → Tasks 1.2–1.3 ✓. Cross-session risk #1 → Phase 0 spike ✓. Risk #2 (headless) → Task 1.1 + 1.3 curl ✓. Risk #3 (cwd canon) → deferred to Part 2 (join is plugin-side) ✓. Plugin/core (§Components/Plugin) → Part 2 (deferred, depends on spike) ✓.

**Placeholder scan:** No TBD/TODO. Every code step shows full code. Two explicit "confirm against existing definition" notes (AgentWatcherSnapshot fields; ts unit) are verification instructions, not placeholders — the implementer adjusts the shown code to the real struct, which they must read.

**Type consistency:** `AgentByCwd` fields are identical across the struct def, the `apply_agent_watcher_snapshot` insert, the test assertions, and `agents_json`. `agents_json(&self) -> String` signature matches the inherent method, the trait default, the trait override, and the `/agents` call site. `default_state_source_from_env` returns `Option<ReadOnlyMuxStateSource>` consistent with Task 1.1's test. Zellij spike APIs (`get_pane_cwd(PaneId::Terminal(u32)) -> Result<PathBuf,String>`, `switch_session_with_focus(&str, None, Some((u32,bool)))`, `Event::SessionUpdate`, `Event::Key`) verified against zellij-tile 0.44 source.
```
