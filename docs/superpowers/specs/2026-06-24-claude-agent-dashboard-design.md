# Claude Code Agent Dashboard (Zellij) — Design

**Date:** 2026-06-24
**Status:** Approved (delegated)
**Builds on:** the `zellij-sidebar` branch (session-level sidebar plugin under `integrations/zellij-plugin/`).

## Goal

A Zellij plugin pane that lists **every Claude Code agent across all Zellij sessions**, each with its **live lifecycle status** (idle / running / tool-running / waiting / done / error), where pressing **Enter** on a row jumps focus to that agent's pane.

The current plugin lists *sessions*; this turns it into an *agent* dashboard.

## Why Approach A (server-backed)

Full lifecycle status cannot be inferred from the pane alone — the plugin can see a pane's title/command/cwd but not whether Claude is mid-tool-call or waiting for input. The `opensessions-server` already derives these exact `AgentStatus` values by parsing Claude transcripts (`packages/runtime-rs/src/agent_watchers.rs`), keyed by the agent's project directory. We reuse that and expose it; the plugin supplies the missing mux truth (which pane is in which cwd).

Rejected alternatives: **B** (Claude hooks → `zellij pipe`, server-free) — real-time but requires per-user `~/.claude/settings.json` config and reimplements status mapping; **C** (hooks → server) — needs both the daemon and hook config. A reuses the most and needs nothing configured per agent.

## Architecture

```
Claude transcripts (~/.claude/projects/<encoded-cwd>/*.jsonl)
        │  (existing watchers, full lifecycle status)
        ▼
opensessions-server  ──  GET /agents  →  [{ cwd, agent, status, threadName,
   (headless mode)                          lastUserPrompt, unseen, ts }]
        ▲                                         │ web_request poll (~1.5s)
        │                                         ▼
        └──────────────  zellij plugin  ─ list Claude panes (SessionUpdate)
                                         ─ get_pane_cwd(pane) per pane
                                         ─ join pane.cwd ⇆ agent.cwd
                                         ─ render status rows; Enter → jump
```

### Join key: canonicalized cwd

Both sides produce a working directory:
- Server: `AgentWatcherSnapshot.project_dir` (the transcript's `cwd`, possibly `__encoded__:`-prefixed).
- Plugin: `get_pane_cwd(PaneId::Terminal(id)) -> PathBuf`.

Match on a canonical form (resolve symlinks where possible, strip trailing slash, normalize). The server already has `encode_agent_project_dir` for the transcript-encoded form; the `/agents` payload returns the **decoded absolute cwd** so the plugin can compare against `get_pane_cwd` output directly.

**Known limitation (v1):** two Claude agents running in the *same* cwd cannot be distinguished by cwd alone, so both rows would show the same status. Acceptable for v1; a later version can disambiguate via Claude session id (requires hooks, i.e. Approach B/C).

## Components

### Server (`apps/server-rs`, `packages/runtime-rs`)

1. **Headless/agent-only startup.** `default_state_source_from_env` returns `None` when `TMUX` is unset, so under Zellij the server has no state source today. Add a path that, when `ZELLIJ_SESSION_NAME` is set (or always as a fallback), starts the server with the agent watchers + HTTP server running and **no mux provider** (mux-dependent endpoints become no-ops). The watcher loops are mux-independent.

2. **Expose cwd-keyed agent status.** The watcher snapshots carry `project_dir`, but `AgentEvent` drops it during mux-session attribution (`apps/server-rs/src/lib.rs:1225`). Retain the decoded cwd alongside each tracked agent and add a read endpoint:
   - `GET /agents` → JSON array, one entry per live Claude (and other-agent) instance: `{ cwd, agent, status, threadName?, lastUserPrompt?, unseen?, ts }`.
   - Sourced from the existing `AgentTracker` / watcher data, keyed by cwd rather than mux session.

### Plugin (`integrations/zellij-plugin`)

Extend the existing `core` + `plugin` crates:

- **core** (host-tested, pure):
  - Add `AgentStatus` enum mirroring the server's (idle/running/tool-running/waiting/done/error) with a parse-from-string.
  - Add `AgentRow { cwd, session, pane_id, agent, status, label, selected }`.
  - Add `join_agents(claude_panes: &[ClaudePane], statuses: &[AgentStatus by cwd]) -> Vec<AgentRow>` — pure cwd-join + selection marking. Unit-tested.
  - Add `canonical_cwd(&str) -> String` normalization. Unit-tested.
- **plugin** (wasm glue):
  - From `SessionUpdate`, collect Claude panes across all sessions (reuse `detect_agent`), recording `(session_name, pane_id, is_focused, cwd)`.
  - Resolve each pane's cwd via `get_pane_cwd` (requires the relevant permission; see risks for cross-session behavior).
  - Poll `GET /agents` via `web_request` on a timer (~1.5s) and on `SessionUpdate`; parse via `WebRequestResult`.
  - Render one row per Claude pane: status glyph/color + a session/cwd label, with j/k selection.
  - Enter → `switch_session_with_focus(&session, None, Some((pane_id, false)))` (cross-session) or `focus_pane_with_id` (same session).

## Data flow (steady state)

1. Plugin receives `SessionUpdate` → rebuilds the Claude-pane list, fires `get_pane_cwd` per pane.
2. Plugin timer fires → `web_request GET /agents`.
3. `WebRequestResult` arrives → plugin parses statuses, `join_agents` against pane cwds → re-render.
4. User presses Enter → jump to the selected pane.

## Error handling

- Server unreachable → rows render with status `unknown`/dimmed and a one-line "server: connecting…" note (the plugin still lists panes from `SessionUpdate`).
- `get_pane_cwd` returns `Err` for a pane → that pane shows `agent` with `unknown` status (still jumpable).
- No Claude panes → empty-state line ("no Claude agents").

## Testing

- **core:** unit tests for `canonical_cwd`, `AgentStatus` parse, and `join_agents` (match, no-match, same-cwd duplicate, selection clamp).
- **server:** unit test for `/agents` serialization keyed by cwd.
- **plugin:** in-Zellij manual verification (build-only acceptance for subagents), exercising: status appears, updates, Enter jumps cross-session.

## Risks — spike before building

1. **Cross-session `get_pane_cwd` / `switch_session_with_focus(pane_id)`** for panes in *other* Zellij sessions. If cross-session cwd/focus is unsupported, v1 scopes to the current session's agents (still useful) and we revisit. **Spike this first.**
2. **Headless server** runs watchers and serves `/agents` with no mux provider.
3. **cwd canonicalization** parity between `get_pane_cwd` output and transcript `project_dir` (symlinks, `/private` on macOS, trailing slash).

## Out of scope (v1)

- Same-cwd multi-agent disambiguation.
- Real-time push (polling is fine for v1; `zellij pipe` push is a later optimization).
- Non-Claude agents (the pipeline supports them, but the dashboard targets Claude Code; others can appear but are not a goal).
- Git/ports enrichment (separate concern).
