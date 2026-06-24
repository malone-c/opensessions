# opensessions Zellij plugin

A native Zellij (WASM) sidebar listing your sessions, with navigation and
session switching. Self-contained: it reads mux state from Zellij's
`SessionUpdate` event and needs no opensessions server.

## Build

Requires the rustup `stable` toolchain (not Homebrew rust) and the wasm target:

    rustup target add wasm32-wasip1
    cd integrations/zellij-plugin
    cargo build -p opensessions-zellij --target wasm32-wasip1 --release

Output: `target/wasm32-wasip1/release/opensessions-zellij.wasm`

> Homebrew's `rust` has no wasm std and shadows rustup on PATH. Run
> `brew unlink rust` once, or prefix builds with `rustup run stable`.

## Load

From inside a Zellij session:

    zellij action launch-or-focus-plugin --skip-plugin-cache \
      file:$PWD/target/wasm32-wasip1/release/opensessions-zellij.wasm

Grant the permission prompt (ReadApplicationState, ChangeApplicationState).

## Keys

- `j` / `Down`, `k` / `Up` — move the selection
- `Enter` — switch to the selected session
- `x` — kill the selected session (not the current one)

## Test

    cargo test -p opensessions-zellij-core

## Not yet supported

Git branch/dirty status, listening ports, and live agent status require the
opensessions server, which under Zellij needs the plugin to feed it mux data —
and Zellij's `PaneInfo` exposes neither pane `cwd` nor `pid`. That is tracked
as a separate follow-up.
