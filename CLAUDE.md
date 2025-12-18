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

# ACP client prototype (standalone test harness)
cd src-tauri/acp
cargo build
RUST_LOG=info cargo run
```

## Architecture Summary

ThoughtTree is a DAG-based conversation tool: React frontend (ReactFlow graph + chat panel) communicates via Tauri IPC with a Rust backend that manages Claude sessions through the Agent Client Protocol.

**Data flow:** User interaction → Tauri command → ACP client → claude-code-acp subprocess → Claude API → streaming response via Tauri events

See README.md for detailed architecture diagrams and component breakdown.

## ACP Integration Notes

The ACP client (prototyped in `src-tauri/acp/`) has critical implementation details:

- **tokio-util compat layer required:** SDK uses `futures-io` traits, not tokio's
- **Non-Send futures:** Use `#[async_trait(?Send)]` and `tokio::task::LocalSet`
- **Connection constructor order:** `ClientSideConnection::new(client, outgoing, incoming, spawn)` — outgoing (stdin) comes before incoming (stdout)
- **Permission handling:** Return `Selected { option_id }` with first option's ID to auto-approve

## Current Development State

App is released and functional. See CHANGELOG.md for version history and README.md for user documentation.

## Development Guidelines

- **Test-Driven Development:** Use TDD where beneficial—write tests before implementation for complex logic, edge cases, and critical paths.
- **Security First:** All changes must improve security or maintain the current level. Never introduce vulnerabilities (XSS, path traversal, command injection, etc.).

## Key Files

- `src-tauri/src/lib.rs` - Tauri app entry point and commands
- `src-tauri/acp/src/main.rs` - ACP client reference implementation
- `src/App.tsx` - React app root
- `src/components/SidePanel/SidePanel.test.tsx` - Example test file
- `src-tauri/tauri.conf.json` - Tauri configuration
