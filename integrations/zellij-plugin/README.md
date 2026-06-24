# opensessions Zellij plugin

A native Zellij (WASM) **Claude Code agent dashboard**: lists the agents running
in the current session with their live lifecycle status (idle / running /
tool-running / waiting / done / error), and jumps focus to an agent's pane on
Enter.

## How it works

- `opensessions-server` runs headless under Zellij (no tmux needed). It detects
  every running `claude` process (by cwd, via `ps`+`lsof`) so idle agents still
  show, and overlays live lifecycle status parsed from Claude transcripts. Served
  at `GET /agents`, keyed by cwd.
- Each agent is labelled with its git **repo** (from the origin remote),
  **branch**, **worktree**, and the **folder** within the worktree where it's
  open — so worktrees and multiple agents in one repo are distinguishable.
- The plugin polls `/agents` and renders one row per agent — so agents in *any*
  Zellij session appear. It reads current-session pane cwds via `get_pane_cwd`;
  when an agent's cwd matches a current-session pane (marked `↵`), Enter calls
  `focus_pane_with_id` to jump to it.

## Build

Requires the rustup `stable` toolchain (not Homebrew rust) and the wasm target:

    rustup target add wasm32-wasip1
    cd integrations/zellij-plugin
    cargo build -p opensessions-zellij --target wasm32-wasip1 --release

Output: `target/wasm32-wasip1/release/opensessions-zellij.wasm`

> Homebrew's `rust` has no wasm std and shadows rustup on PATH. Run
> `brew unlink rust` once, or prefix builds with `rustup run stable`.

## Run

1. Start the server (headless agent tracker) — it auto-detects Zellij:

       cargo build --release -p opensessions-server
       ./target/release/opensessions-server &

2. Load the plugin from inside a Zellij session:

       zellij action launch-or-focus-plugin --skip-plugin-cache \
         file:$PWD/target/wasm32-wasip1/release/opensessions-zellij.wasm \
         --configuration server_url=http://127.0.0.1:7391

   Grant the permission prompt (ReadApplicationState, ChangeApplicationState,
   WebAccess).

## Keys

- `j` / `Down`, `k` / `Up` — move the selection
- `Enter` — jump focus to the selected agent's pane (only rows marked `↵`, whose
  pane is in the current session — Zellij can't focus panes in other sessions)

## Test

    cargo test -p opensessions-zellij-core

## Limitations

- Multiple Claude agents in the **same** cwd collapse to one row (cwd is the
  join key; per-session disambiguation would need agent session ids).
- Agents whose pane is in another Zellij session show status but aren't jumpable
  (Zellij doesn't expose other sessions' panes to a plugin).
