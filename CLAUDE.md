# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

```bash
# Full app development
pnpm install          # Install dependencies
pnpm tauri dev        # Run app in development mode
pnpm tauri build      # Build production app

# Frontend only
pnpm dev              # Vite dev server (port 1420)
pnpm build            # TypeScript check + Vite build

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

Check README.md "Current State" section for task status. The ACP prototype is working; main app integration is in progress.

## Key Files

- `src-tauri/acp/src/main.rs` - Working ACP client reference implementation
- `src-tauri/src/lib.rs` - Tauri app entry point and commands
- `src/App.tsx` - React app root
- `tauri.conf.json` - Tauri configuration (ports, build commands, window settings)
