# AIQ

A private coach for how you work with AI agents. It watches your prompts on your own machine and, at a
natural pause, shows one short tip. Nothing leaves the device.

**Phase 1 (walking skeleton, Claude Code only):** reword the same request three times without adding
anything new, and one card appears under Claude's reply:

```
⎿  Stop says: AIQ tip: you've reworded this request 3 times without adding new details.
   Why: rewording alone rarely changes the result; the agent needs something new to go on.
   Try this: say what was wrong with the last answer, and add one example, file or constraint.
   Details: aiq show 01…  ·  Turn off tips for now: aiq pause
```

At most one card per Claude Code session, at most 5 a day (your setting).

## Build

Rust 1.89 or later. No other tools are needed (the store is plain SQLite, compiled in).

```
cargo build -p aiq --release      # target/release/aiq (aiq.exe on Windows)
cargo test --workspace            # unit, store, spool-replay property and end-to-end tests
```

CI builds and tests on Linux and Windows and uploads `aiq.exe` as the `aiq-windows-x64` artifact
(`.github/workflows/rust.yml`).

**AIQ home** (the desktop app, Tauri + React, `apps/home`) needs Node 22 as well:

```
cd apps/home && npm ci
npm test && npm run build && npm run e2e   # unit, then UI end-to-end, keyboard and axe checks
npx tauri build --no-bundle               # src-tauri/target/release/aiq-home(.exe)
```

CI uploads `aiq-home.exe` as the `aiq-home-windows-x64` artifact (`.github/workflows/home.yml`). The app reads
your store read-only and sends pause, settings and delete through the running core.

## Use

```
aiq install claude-code     # shows the change to ~/.claude/settings.json, asks, keeps a backup
aiq status                  # running or paused, tips today, store size, hook times
aiq show <card-id>          # a card, why it showed, and the prompts it counted
aiq pause | aiq resume
aiq config set daily-cap 3
aiq import --dry-run        # count your past Claude Code / Copilot CLI sessions; aiq import stores them
aiq forget --imported       # delete only what aiq import added
aiq forget --all            # delete everything AIQ stored on this machine
aiq uninstall claude-code   # removes only AIQ's two hook entries
```

The hooks call `aiq` directly (no shell). The background core starts on your next prompt.

**Where data lives:** `%USERPROFILE%\.aiq` on Windows (not AppData: the Claude desktop app is a packaged app whose AppData writes go to a private copy); `~/.local/share/aiq` on Linux and macOS. Files are
readable only by you. **Phase 1 stores prompts unencrypted** (encryption deferred; see
`docs/notes/note-20261031-encryption-deferred.md`). Logs never contain prompt text.

## Layout

| Path | What |
|---|---|
| `crates/aiq-contracts` | `aiq.event.v1`, `aiq.card.v1`, local-API frames, pack schema, error codes |
| `crates/aiq-domain` | Pure logic: Task rule, re-prompt detector, trigger engine, card composer (no I/O) |
| `crates/aiq-store` | SQLite schema v1: dimensions + append-only facts, migrations |
| `crates/aiq-core` | Paths, user-only socket / named pipe, spool, single-writer decision loop, bundled pack |
| `crates/aiq-home` | AIQ home read model: one snapshot of the store (read-only) for the desktop app |
| `crates/aiq` | The binary: hook client, core, CLI |
| `apps/home` | AIQ home desktop app: React UI (`src`), Tauri shell (`src-tauri`), UI tests (`tests`) |
| `contracts/` | JSON Schemas |
| `docs/` | Specs, architecture, ADRs, designs, Proof Packs |
