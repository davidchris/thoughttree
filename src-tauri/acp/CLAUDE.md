# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ACP Client Prototype - a Rust CLI that connects to Claude Code via the Agent Client Protocol (ACP). This validates the integration pattern for building a graph-style LLM conversation tool.

## Build Commands

```bash
cargo build              # Build the project
cargo run                # Run the CLI
RUST_LOG=debug cargo run # Run with debug logging
```

## Architecture

**Subprocess-based ACP integration:**
- Spawns `claude-code-acp` (npm: `@zed-industries/claude-code-acp`) as a subprocess
- Communicates via stdin/stdout using the `agent-client-protocol` Rust SDK
- Implements the `Client` trait for handling permission requests and session notifications

**Core flow:**
1. Spawn `npx @zed-industries/claude-code-acp`
2. Connect via `ClientSideConnection`
3. Send `initialize` request with client capabilities
4. Create session via `session/new`
5. Send prompts via `session/prompt`
6. Stream responses through `SessionNotification` callbacks

## Key Dependencies

- `agent-client-protocol` - ACP Rust SDK
- `tokio` - async runtime (requires `full` + `process` features)
- `async-trait` - for implementing the async `Client` trait

## Prerequisites

Before running, ensure:
1. Claude Code CLI installed: `npm install -g @anthropic-ai/claude-code`
2. Authenticated: `claude login`
3. ACP adapter available: `npx @zed-industries/claude-code-acp`

## Resources

- SDK examples: https://github.com/agentclientprotocol/rust-sdk/tree/main/rust/examples
- SDK docs: https://docs.rs/agent-client-protocol/latest/agent_client_protocol/
- Protocol overview: https://agentclientprotocol.com/overview/introduction
