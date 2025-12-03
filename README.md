# ThoughtTree

A graph-based conversation tool for LLMs. Think mind-map meets chat interface — branch conversations, explore ideas in parallel, and keep your research organized as a DAG rather than linear chat threads.

## Why

Linear chat interfaces force sequential thinking. When doing R&D or exploring complex topics, you often want to:
- Branch a conversation to explore "what if" scenarios
- Return to earlier points and try different directions  
- Compare responses across branches
- Keep context visible across related threads

ThoughtTree treats conversations as a directed acyclic graph (DAG) where each node is a conversation state you can branch from.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         ThoughtTree                             │
├─────────────────────────────────────────────────────────────────┤
│  React Frontend                                                 │
│  ├── ReactFlow (DAG visualization)                              │
│  ├── Zustand (state management)                                 │
│  └── Chat panel (message display + input)                       │
├─────────────────────────────────────────────────────────────────┤
│  Tauri Backend (Rust)                                           │
│  ├── ACP client (Agent Client Protocol)                         │
│  ├── Session management                                         │
│  └── Tauri commands (IPC bridge)                                │
├─────────────────────────────────────────────────────────────────┤
│  claude-code-acp (Node.js subprocess)                           │
│  └── Connects to Claude via user's Max subscription             │
└─────────────────────────────────────────────────────────────────┘
```

### Key Integration: Agent Client Protocol (ACP)

We use ACP to communicate with Claude Code, which allows users to leverage their existing Claude Max subscription. The protocol is JSON-RPC over stdio — we spawn `claude-code-acp` as a subprocess and exchange messages.

**ACP flow:**
1. Spawn `npx @zed-industries/claude-code-acp`
2. Initialize connection with protocol version and capabilities
3. Create session with working directory
4. Send prompts, receive streaming responses via session notifications
5. Handle permission requests for tool calls

## Tech Stack

- **Frontend:** React 18, TypeScript, Vite
- **Graph UI:** @xyflow/react (ReactFlow v12)
- **State:** Zustand
- **Desktop:** Tauri 2.0 (Rust backend)
- **LLM Integration:** Agent Client Protocol → Claude Code → Claude Max

## Project Structure

```
thoughttree/
├── src/                    # React frontend
│   ├── components/
│   │   ├── Graph/          # ReactFlow canvas + custom nodes
│   │   └── ChatPanel/      # Conversation UI
│   ├── store/              # Zustand stores
│   ├── types/              # TypeScript types
│   └── lib/                # Tauri API wrappers
├── src-tauri/              # Rust backend
│   └── src/
│       ├── main.rs
│       ├── commands.rs     # Tauri commands
│       └── acp/            # ACP client module
│           ├── mod.rs
│           ├── client.rs   # Client trait impl
│           └── connection.rs
├── package.json
├── tauri.conf.json
└── TASK-02-graph-ui-integration.md  # Current implementation task
```

## Current State

### Completed
- [x] Tauri + React project scaffold
- [x] ACP client prototype (standalone, proven working)
  - Spawns claude-code-acp subprocess
  - Handles initialize → new_session → prompt flow
  - Streams responses via session notifications
  - Auto-approves permission requests

### In Progress
- [ ] **Task 2:** Graph UI and ACP integration (see `TASK-02-graph-ui-integration.md`)

### Planned
- [ ] Persistence (save/load conversation graphs)
- [ ] Deep research mode (parallel ACP sessions as subagents)
- [ ] Markdown export
- [ ] Tool permission UI (instead of auto-approve)

## Development

### Prerequisites

1. Node.js and pnpm
2. Rust toolchain
3. Claude Code CLI: `npm install -g @anthropic-ai/claude-code`
4. Authenticated with Claude: `claude login`

### Run

```bash
pnpm install
pnpm tauri dev
```

### ACP Prototype Reference

The working ACP client prototype is in `../acp-client-prototype/` (or wherever you have it). Key learnings from that implementation:

- SDK uses `futures-io` traits — need `tokio-util` compat layer
- `#[async_trait(?Send)]` required for Client trait (non-Send futures)
- `ClientSideConnection::new(client, outgoing, incoming, spawn)` — outgoing first!
- Permission responses need `Selected { option_id }` with first option ID
- Must run in `tokio::task::LocalSet` due to SDK constraints

## Key Technical Decisions

1. **ACP over direct API:** Allows using Claude Max subscription instead of paying for API separately. User authenticates via Claude Code CLI.

2. **Tauri over Electron:** Smaller binaries (~10MB vs 150MB+), Rust backend for performance, native feel.

3. **ReactFlow for graph:** Battle-tested DAG visualization, good React integration, handles pan/zoom/selection.

4. **Zustand over Redux:** Simpler API for this use case, works well with ReactFlow's controlled components.

5. **Streaming via Tauri events:** Backend emits `stream-chunk` events, frontend subscribes per-node. Decouples async streaming from invoke/response cycle.

## Resources

- [Agent Client Protocol](https://agentclientprotocol.com/)
- [ACP Rust SDK](https://docs.rs/agent-client-protocol/)
- [ReactFlow Docs](https://reactflow.dev/)
- [Tauri v2 Docs](https://v2.tauri.app/)
- [Anthropic Multi-Agent Research](https://www.anthropic.com/engineering/multi-agent-research-system)
