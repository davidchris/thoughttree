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

To use ThoughtTree, you need:

1. **Claude Max subscription** — Required for accessing Claude models
2. **Claude Code installed** — [Installation guide](https://code.claude.com/docs/en/overview)

   ```bash
   # macOS / Linux
   curl -fsSL https://claude.ai/install.sh | bash
   ```

   Run `claude` once to authenticate with your Claude account. You only need to do this once.

**Alternative:** Set the `ANTHROPIC_API_KEY` environment variable if you prefer using an API key directly.

## Download and Install

ThoughtTree is currently available for macOS:

*   **macOS:** Download the `.dmg` from [Releases](https://codeberg.org/dcwilde/thoughttree/releases)

After downloading:
1. Double-click the `.dmg` file and drag ThoughtTree to your Applications folder
2. **First launch:** Right-click the app → "Open" (required once for unsigned apps)

## Build from Source

If you prefer to build ThoughtTree yourself:

1. Install [Bun](https://bun.sh) and [Rust](https://rustup.rs/)
2. Clone and build:

```bash
git clone https://codeberg.org/dcwilde/thoughttree.git
cd thoughttree
bun install
bun run build:sidecar
bun run tauri:build
```

The built app will be in `src-tauri/target/release/bundle/`.

## Getting Started

On first launch, ThoughtTree will prompt you to select a **notes directory** — this is where your `.thoughttree` files are saved and where Claude can read files (via `@/path` mentions).

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
│  claude-code-acp (bundled sidecar)                              │
│  └── Connects to Claude via user's Max subscription             │
└─────────────────────────────────────────────────────────────────┘
```

ThoughtTree uses the [Agent Client Protocol (ACP)](https://agentclientprotocol.com/) to communicate with Claude Code. This allows users to leverage their existing Claude Max subscription — no separate API costs.

## Privacy

ThoughtTree does not collect any telemetry, analytics, or user data. Your conversations and files stay on your machine.

The only external communication is with Anthropic's Claude API through the bundled Claude Code integration. Your prompts and file contents (when using `@/path` mentions) are sent to Claude for processing. See [Anthropic's Privacy Policy](https://www.anthropic.com/privacy) and [Claude Code documentation](https://code.claude.com/docs/en/overview) for details on how Anthropic handles your data.

## Resources

- [Agent Client Protocol](https://agentclientprotocol.com/)
- [ReactFlow Docs](https://reactflow.dev/)
- [Tauri v2 Docs](https://v2.tauri.app/)
