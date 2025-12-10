# ThoughtTree

A graph-based conversation tool for LLMs. Think mind-map meets chat interface — branch conversations, explore ideas in parallel, and keep your research organized as a DAG rather than linear chat threads.

## Why

Linear chat interfaces force sequential thinking. When doing R&D or exploring complex topics, you often want to:
- Branch a conversation to explore "what if" scenarios
- Return to earlier points and try different directions  
- Compare responses across branches
- Keep context visible across related threads

ThoughtTree treats conversations as a directed acyclic graph (DAG) where each node is a conversation state you can branch from.

## Prerequisites

To use ThoughtTree, you will need:
*   **Node.js**: Includes `npm` (Node Package Manager), which is used to install the Claude Code CLI.
*   **Claude Code CLI**: This command-line tool facilitates communication between ThoughtTree and Claude.
*   **Claude Max subscription**: Required for accessing advanced Claude models via the Claude Code CLI.

## Download and Install ThoughtTree

ThoughtTree is available for macOS, Windows, and Linux. Please download the appropriate installer for your operating system:
*   **macOS:** Download the `.dmg` from TBD
*   **Windows:** Download the `.exe` from TBD
*   **Linux:** Download the `.AppImage` or `.deb` from TBD

After downloading, install the application:
*   **macOS:** Double-click the `.dmg` file and drag ThoughtTree to your Applications folder.
*   **Windows:** Run the `.exe` installer and follow the on-screen prompts.
*   **Linux:** Execute the `.AppImage` (you might need to make it executable first, e.g., `chmod +x YourApp.AppImage`) or install the `.deb` package.

## Initial Configuration

After installing ThoughtTree, you need to set up your environment to connect to Claude.

### 1. Install Node.js

Node.js provides the `npm` (Node Package Manager) command, which is necessary for the next step.
*   **macOS:** If you have Homebrew, open your Terminal and run:
    ```bash
    brew install node
    ```
*   **Other OS or without Homebrew:** Download and install Node.js from [https://nodejs.org](https://nodejs.org). The installer will typically include `npm` and add it to your system's PATH.

### 2. Install Claude Code CLI

This tool allows ThoughtTree to interact with Claude through your existing Claude Max subscription.
Open your Terminal (macOS/Linux) or Command Prompt/PowerShell (Windows) and run:
```bash
npm install -g @anthropic-ai/claude-code
```

### 3. Authenticate with Claude

This step connects the Claude Code CLI to your Claude account. It will open a browser window for you to log in and confirm your Claude Max subscription.
In your Terminal or Command Prompt, run:
```bash
claude login
```

On first launch, ThoughtTree will prompt you to select a notes directory — this is where your `.thoughttree` files are saved and where Claude can read files (via `@/path` mentions).

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
1. Spawn `npx @zed-industries/claude-code-acp` subprocess
2. Initialize connection with protocol version and capabilities
3. Create session with working directory
4. Send prompts, receive streaming responses via Tauri events
5. Handle permission requests with smart filtering:
   - **Auto-deny:** Bash, Write, Edit, TodoWrite, Task (safety enforcement)
   - **Auto-approve:** Read, Grep, Glob, WebSearch, Skill (within notes directory)
   - **Prompt user:** WebFetch (per-session approval)

**Backend implementation:**
- Non-Send async with `#[async_trait(?Send)]` and `tokio::task::LocalSet`
- Proper stream handling with Tokio's AsyncRead/AsyncWrite adapters
- Permission filtering by project notes directory path
- Session isolation per conversation thread

## Tech Stack

- **Frontend:** React 18, TypeScript, Vite
- **Graph UI:** @xyflow/react (ReactFlow v12) — DAG visualization with custom nodes
- **Rendering:** react-markdown + remark-gfm, mermaid (diagrams), react-syntax-highlighter (code blocks)
- **State:** Zustand — graph state, persistence, auto-save
- **Desktop:** Tauri 2.0 (Rust backend) — IPC, file I/O, plugin system
- **Plugins:** tauri-plugin-store (config), tauri-plugin-dialog (file dialogs)
- **LLM Integration:** Agent Client Protocol (ACP) → Claude Code → Claude Max

## Project Structure

```
thoughttree/
├── src/                    # React frontend (Vite + TypeScript)
│   ├── components/
│   │   ├── Graph/          # ReactFlow canvas, custom nodes, snapping, alignment guides
│   │   ├── ChatPanel/      # Chat interface with streaming message display
│   │   ├── FileAutocomplete/   # @ mention autocomplete
│   │   ├── MarkdownContent.tsx # Syntax highlighting, Mermaid rendering
│   │   ├── SidePanel.tsx   # Resizable content editor
│   │   ├── PermissionDialog.tsx # WebFetch approval UI
│   │   ├── SetupWizard.tsx # First-run configuration
│   │   └── Toolbar.tsx     # Project management controls
│   ├── store/              # Zustand state management
│   │   └── useGraphStore.ts    # Graph state + persistence
│   ├── types/              # TypeScript type definitions
│   └── lib/                # Tauri API wrappers, utilities
├── src-tauri/              # Rust backend (Tauri 2.0)
│   ├── src/
│   │   └── lib.rs          # All Tauri commands + ACP client integration (775 lines)
│   │       ├── ACP client implementation
│   │       ├── Session & streaming management
│   │       ├── Permission system with path filtering
│   │       └── Project I/O commands
│   └── acp/                # Standalone ACP prototype (reference implementation)
│       └── src/main.rs
├── package.json
├── tauri.conf.json
└── pnpm-lock.yaml
```

## Current State

### Completed
- [x] Tauri + React project scaffold
- [x] **Graph UI with ReactFlow** — Full DAG visualization with custom nodes, snapping, alignment guides
  - Collapsible preview mode with 30-char summaries
  - Double-click to edit user nodes
  - Context menu (reply, delete)
  - Keyboard shortcuts (Enter, Space, Escape)
  - Minimap, background grid, node resizing
- [x] **ACP Integration** — Fully integrated into main app (production-ready, not prototype)
  - Spawns claude-code-acp subprocess
  - Complete initialize → new_session → prompt flow
  - Streaming responses via Tauri events
  - Smart permission system with path-based filtering
- [x] **Chat Panel & Streaming** — Rich message display with generation controls
- [x] **Markdown + Mermaid Rendering** — Syntax highlighting, dark theme diagrams
- [x] **File Mention System** — `@/path/to/file` autocomplete with fd integration
- [x] **Project Persistence** — Save/load conversation graphs as `.thoughttree` files
- [x] **Export Functionality** — Export thread (to selected node) or entire graph as Markdown
- [x] **Side Panel** — Editable content preview with resize handle
- [x] **Toolbar & Project Management** — New/Open/Save/Export with setup wizard
- [x] **Permission System** — Directory-scoped tool access, WebFetch approval dialogs

### Planned
- [ ] Deep research mode (parallel ACP sessions as subagents)
- [ ] Enhanced permission UI for all tool types (currently used for WebFetch)

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

### ACP Implementation Reference

The ACP client is fully integrated into `src-tauri/src/lib.rs` (production code). The `src-tauri/acp/` directory contains a standalone reference implementation for testing and debugging the ACP protocol independently.

**Key implementation details** (from both prototype and production code):

- SDK uses `futures-io` traits — requires `tokio-util` compat layer
- `#[async_trait(?Send)]` required for Client trait (non-Send futures)
- `ClientSideConnection::new(client, outgoing, incoming, spawn)` — outgoing (stdin) before incoming (stdout)
- Permission responses use `Selected { option_id }` with appropriate option ID
- Must run in `tokio::task::LocalSet` due to SDK non-Send constraint
- Stream framing with `tokio::io::split()` and AsyncRead/AsyncWrite adapters

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
