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

ThoughtTree supports macOS. Codex is the default provider. Claude Code is also available.

For Codex, install Node.js and the ACP adapter:

```bash
npm install -g @agentclientprotocol/codex-acp@latest
codex-acp login
```

The adapter includes a compatible Codex CLI. It uses your Codex authentication.
The model menus read the models available to your account from the adapter.
See the [Codex model documentation](https://learn.chatgpt.com/docs/models) for model availability.

For Claude Code, follow the [installation guide](https://code.claude.com/docs/en/overview), then run `claude` to authenticate.

Gemini is no longer an available provider. Saved Gemini messages remain readable.

## Download and Install

ThoughtTree is currently available for macOS:

* **macOS:** Download the `.dmg` from [GitHub Releases](https://github.com/davidchris/thoughttree/releases)

After downloading:

1. Double-click the `.dmg` file.
2. Drag ThoughtTree to the Applications folder.
3. Right-click ThoughtTree and select **Open** on the first launch. The app is not signed.

## Build from Source

If you prefer to build ThoughtTree yourself:

1. Install [Bun](https://bun.sh) and [Rust](https://rustup.rs/)
2. Clone and build:

```bash
git clone https://github.com/davidchris/thoughttree.git
cd thoughttree
bun install
bun run build:sidecar
bun run tauri:build
```

The built app will be in `target/release/bundle/`.

## Getting Started

On first launch, ThoughtTree will prompt you to select a **notes directory** — this is where your `.thoughttree` files are saved and where your selected agent can read files (via `@/path` mentions).

## File nodes

A file node puts a file from your notes directory on the canvas, so one file can anchor several independent branches of conversation.

**Add a file.** Drag a file from your notes directory onto the canvas, or right-click the canvas and choose "Add file…". Only files inside the notes directory can be linked; the picker and the drop target refuse anything outside it. One file per node — add several nodes for several files.

**Preview.** The card shows a type badge (PNG, MD, RS, … or FILE), the file name and size, and a preview: a thumbnail for PNG, JPG, GIF, and WebP; the first 16 KB for text-like files (Markdown, code, JSON, YAML, CSV, …). Other types show only the badge, name, and size. Double-click the card to open the file in the side panel.

**Use it as context.** Connect the file node to a user node. Every node downstream receives the file. File nodes have no incoming edges — they are a source, not a reply.

**How the file reaches the agent.**

- Images are read from disk when you send and delivered inline. Limits: 5 MB per image, 8000 px on the longest side, 20 images per prompt. A larger image shows "too large for the agent" and blocks sending on that branch.
- Every other file is delivered as a pointer (path, type, size). The agent reads the file with its own tools, through the usual permission prompts. There is no size limit.

**Always fresh.** The node stores only the path, never the file content, so the agent always gets the file as it is on disk at send time. When the file changes, the card shows "changed on disk"; click Refresh to adopt the current version and update the preview. A deleted or moved file shows "file missing" and blocks sending until you restore the file or delete the node.

Project files are saved in format v5 once this version runs; older ThoughtTree builds cannot open v5 files.

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
│  ACP adapters: codex-acp or bundled claude-code-acp              │
│  └── Connect to the selected provider                           │
└─────────────────────────────────────────────────────────────────┘
```

ThoughtTree uses the [Agent Client Protocol (ACP)](https://agentclientprotocol.com/) to communicate with Codex and Claude Code.
Automatic node headings use the default provider: Codex Luna or Claude Haiku.

## Privacy

ThoughtTree does not collect telemetry or analytics. Project files remain in your notes directory.
Your selected provider receives prompts and attached file content for processing.
Provider CLI configuration also applies to ThoughtTree sessions.

## Codex connection check

The integration was checked with `@agentclientprotocol/codex-acp` 1.11.0 on 2026-09-14.
The Rust ACP SDK remains at 0.9.2 because it supports the protocol used by this adapter.
The newer 2.x Rust SDK changes the client API and is not required for this connection.

The app adds known runtime directories to the adapter PATH for macOS desktop launches.
Current adapters receive model and reasoning effort through `CODEX_CONFIG`.
Legacy adapters receive equivalent `-c` flags.
The app uses live model discovery, with a static catalog for adapters that return no models.

To check discovery, streaming, and node headings with your existing Codex login, run:

```bash
cargo run -p thoughttree-core --example codex_smoke
```

This check sends two small prompts from a temporary directory.

## Resources

- [Agent Client Protocol](https://agentclientprotocol.com/)
- [ReactFlow Docs](https://reactflow.dev/)
- [Tauri v2 Docs](https://v2.tauri.app/)
