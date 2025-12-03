# ACP Client Prototype

Minimal Rust CLI that connects to Claude Code via the Agent Client Protocol (ACP). Foundation for a mind-map / thought exploration app using LLMs.

## Architecture

```
┌─────────────────┐    stdin/stdout    ┌──────────────────────┐
│  Rust Client    │◄──────────────────►│  claude-code-acp     │
│  (this app)     │    JSON-RPC        │  (npm subprocess)    │
└─────────────────┘                    └──────────────────────┘
```

**Flow:** spawn subprocess → initialize → new_session → prompt → stream response

## Key SDK Types

- `ClientSideConnection` - manages bidirectional JSON-RPC communication
- `Client` trait - implement `request_permission` and `session_notification`
- `SessionUpdate::AgentMessageChunk` - streaming response chunks
- `tokio::task::LocalSet` - required for non-Send futures from SDK

## Prerequisites

1. Node.js/npm installed
2. Claude Code CLI: `npm install -g @anthropic-ai/claude-code`
3. Authenticated: `claude login`

## Run

```bash
RUST_LOG=info cargo run
```

## Sample Output

```
INFO  Spawning claude-code-acp...
INFO  Creating ACP connection...
INFO  Initializing connection...
INFO  Connected to agent: "@zed-industries/claude-code-acp" v0.10.9 (protocol: 1)
INFO  Creating session...
INFO  Session created: 019acac3-393c-76ad-a769-4f252aff2abd
INFO  Sending prompt...

--- Response ---
The Agent Client Protocol (ACP) is a standardized communication protocol...
--- End Response ---

INFO  Stop reason: EndTurn
INFO  Shutting down...
```

## Learnings

- SDK uses `futures-io` traits, need `tokio-util` compat layer for tokio streams
- `#[async_trait(?Send)]` required - SDK uses non-Send futures for LocalSet
- `ClientSideConnection::new(client, outgoing, incoming, spawn)` - outgoing first!
- Permission handling: return `Selected { option_id }` with first option to auto-approve
