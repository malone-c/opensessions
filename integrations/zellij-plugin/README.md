# opensessions Zellij plugin

A native Zellij (WASM) **Claude Code agent dashboard**: lists the agents running
in the current session with their live lifecycle status (idle / running /
tool-running / waiting / done / error), and jumps focus to an agent's pane on
Enter.

## How it works

- `opensessions-server` runs headless under Zellij (no tmux needed) and parses
  Claude transcripts into per-cwd agent status, served at `GET /agents`.
- The plugin polls `/agents`, reads each pane's cwd via `get_pane_cwd`, and joins
  them on cwd. Enter calls `focus_pane_with_id` to jump to the matched pane.

Scoped to the current session — Zellij's `SessionUpdate` only surfaces the
current session's panes to a plugin.

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
- `Enter` — jump focus to the selected agent's pane

## Test

    cargo test -p opensessions-zellij-core

## Limitations

- Two Claude agents in the **same** cwd can't be told apart by cwd alone.
- Current session only (Zellij does not expose other sessions' panes to a plugin).
