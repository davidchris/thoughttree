# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

```bash
# Full app development
bun install           # Install dependencies
bun run tauri dev     # Run app in development mode
bun run tauri:build   # Build production app (includes sidecar)

# Frontend only
bun run dev           # Vite dev server (port 1420)
bun run build         # TypeScript check + Vite build

# Testing
bun test              # Run tests in watch mode
bun test:run          # Run tests once

# Before committing Rust changes (CI enforces all four)
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/check-core-no-tauri.sh
```

## Architecture Summary

ThoughtTree is a DAG-based conversation tool: React frontend (ReactFlow graph + chat panel) communicates via Tauri IPC with a Rust backend that manages Claude sessions through the Agent Client Protocol.

**Data flow:** User interaction → Tauri command → ACP client → claude-code-acp subprocess → Claude API → streaming response via Tauri events

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

## Code Search

Structural search: `sg -lang rust -p 'pattern'` for syntax-aware matching across Rust/TypeScript. Faster than text grep for finding function defs, imports, type patterns.

## Development Guidelines

- **Test-Driven Development:** Use TDD where beneficial—write tests before implementation for complex logic, edge cases, and critical paths.
- **Security First:** All changes must improve security or maintain the current level. Never introduce vulnerabilities (XSS, path traversal, command injection, etc.).
- Boy-scout rule: leave the code better than you found it

## Key Files

- `src-tauri/src/lib.rs` - Tauri app entry point and commands
- `crates/thoughttree-core/src/acp/` - ACP client (sessions, clients, session setup, process spawning)
- `src/App.tsx` - React app root
- `src/components/SidePanel/SidePanel.test.tsx` - Example test file
- `src-tauri/tauri.conf.json` - Tauri configuration
