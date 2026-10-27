# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

```bash
# Prove a change: every check CI requires (format, lint, typecheck, tests)
just verify

# Native app
cargo run --locked -p thoughttree-gpui   # Run the app in development mode
bun run build:sidecar                    # Build the Claude sidecar into binaries/ (Claude Code provider)
scripts/build-gpui.sh                    # Build target/release/bundle/ThoughtTree.app (--debug for a debug bundle)
scripts/build-dmg.sh                     # Package the release bundle as the DMG

# Before committing TypeScript changes in packages/graph-model (CI enforces all three; lint runs with --max-warnings 0)
bun install
bun run typecheck
bun run lint
bun run test:run

# Before committing Rust changes (CI enforces all three)
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Lint Rules

ESLint (`eslint.config.js`, type-aware via typescript-eslint) and Clippy (`clippy.toml`) enforce nesting and complexity budgets:

- `max-depth` 3, `complexity` 20 (modified variant), `no-else-return` — warnings locally, but CI fails on any warning
- `@typescript-eslint/no-floating-promises`, `no-misused-promises` — errors
- `clippy::excessive_nesting` at threshold 5 — counts fn/impl/mod blocks, not just branches
- Test files are exempt from the complexity budget, not from the promise rules
- Suppressions need a reason: `// eslint-disable-next-line <rule> -- <why>`

## Architecture Summary

ThoughtTree is a DAG-based conversation tool: a native GPUI app (`crates/thoughttree-gpui`) edits the Graph through a pure Rust model (`crates/thoughttree-gpui-model`) and calls in-process desktop services (`crates/thoughttree-desktop`), which run provider sessions in `crates/thoughttree-core` through the Agent Client Protocol.

**Data flow:** User interaction → GPUI view → `Desktop` service → ACP client → provider adapter subprocess (codex-acp or the bundled claude-code-acp) → streaming `DesktopEvent`s back to the view

See README.md for detailed architecture diagrams and component breakdown.

## ACP Integration Notes

The ACP client (`crates/thoughttree-core/src/acp/`) has critical implementation details:

- **SDK 2.x builder model:** `Client.builder().on_receive_notification(..).on_receive_request(..).connect_with(transport, main_fn)`; schema types live under `agent_client_protocol::schema::v1`
- **tokio-util compat layer required:** transport is `ByteStreams::new(stdin.compat_write(), stdout.compat())` — SDK uses `futures-io` traits, not tokio's
- **Handlers must be `Send`:** `SessionClient` (our trait) uses `#[async_trait]`; sessions still run on a `tokio::task::LocalSet` for the stderr logger
- **Dispatch loop:** handlers block further message processing; permission prompts are answered from `cx.spawn` so streaming continues while the user decides
- **Model switching:** `session/set_model` is gone; use `SetSessionConfigOptionRequest` with config id `model` when `session/new` advertises a model config option
- **Permission handling:** Return `Selected { option_id }` with first option's ID to auto-approve

## Current Development State

App is released and functional. See CHANGELOG.md for version history and README.md for user documentation.

**One desktop app: GPUI.** The native app (`docs/gpui/README.md`) is the only desktop frontend. The Tauri app had its final release in v0.6.0 and left the repository (ADR 0010).

- Releases ship a macOS arm64 DMG through `.github/workflows/release.yml`.
- Linux and Windows users stay on Tauri v0.6.0 until GPUI packages exist. Projects that GPUI saves must still open there: `packages/graph-model` stays as the v0.6.0 Project reader, and `crates/thoughttree-gpui-model/tests/parity.rs` round-trips a GPUI save through it with Bun. A Project format change that v0.6.0 cannot read needs its own ADR.

## Code Search

Structural search: `sg -lang rust -p 'pattern'` for syntax-aware matching across Rust/TypeScript. Faster than text grep for finding function defs, imports, type patterns.

## Development Guidelines

- **Test-Driven Development:** Use TDD where beneficial—write tests before implementation for complex logic, edge cases, and critical paths.
- **Security First:** All changes must improve security or maintain the current level. Never introduce vulnerabilities (XSS, path traversal, command injection, etc.).
- Boy-scout rule: leave the code better than you found it

## Key Files

- `crates/thoughttree-gpui/src/main.rs` - Native app entry point
- `crates/thoughttree-gpui/src/workspace.rs` - `Workspace`, the main window view
- `crates/thoughttree-gpui-model/src/` - Graph, Editor and undo, Tidy layout, search, import, Project format
- `crates/thoughttree-desktop/src/lib.rs` - `Desktop` service and the `DesktopEvent`s it sends to the app
- `crates/thoughttree-core/src/acp/` - ACP client (sessions, clients, session setup, process spawning)
- `crates/thoughttree-gpui/src/interaction_tests.rs` - Example native interaction tests
- `scripts/build-gpui.sh` - App bundle: executable, sidecar, icon, Info.plist

## Agent skills

### Issue tracker

GitHub Issues on `davidchris/thoughttree`, via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five roles: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: root `GLOSSARY.md` plus `docs/adr/`. See `docs/agents/domain.md`.

### Workflow

Definition of Ready and Done, evidence rules, filing rule, and merge gate. See `docs/agents/workflow.md`.
