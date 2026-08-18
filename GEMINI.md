# GEMINI.md - Context for ThoughtTree

## Project Overview

**ThoughtTree** is a graph-based conversation tool for LLMs. It treats conversations as a Directed Acyclic Graph (DAG) rather than linear threads, allowing users to branch ideas, explore "what if" scenarios, and organize research visually.

The application is built as a desktop app using **Tauri 2.0** (Rust) and **React 18** (TypeScript). It leverages the **Agent Client Protocol (ACP)** to communicate with Anthropic's Claude via the `claude-code-acp` subprocess, utilizing the user's existing Claude Max subscription.

## Architecture

The application consists of three main components:

1.  **React Frontend:**
    *   Built with Vite, React 18, and TypeScript.
    *   Uses **ReactFlow (v12)** for the DAG visualization.
    *   Uses **Zustand** for state management (graph state, persistence).
    *   Renders Markdown (react-markdown) and diagrams (mermaid).

2.  **Tauri Backend (Rust):**
    *   Handles IPC (Inter-Process Communication) with the frontend.
    *   Manages the **ACP client** integration.
    *   Handles file I/O (project saving/loading) and session management.
    *   Implements a permission system for tool access (Read, Write, WebSearch, etc.).

3.  **Agent Client Protocol (ACP):**
    *   Communicates with the `claude-code-acp` Node.js subprocess via JSON-RPC over stdio.
    *   Connects to Claude using the user's local authentication (`claude login`).

## Building and Running

### Prerequisites

*   **Node.js** (and `pnpm`)
*   **Rust** toolchain
*   **Claude Code CLI:** `npm install -g @anthropic-ai/claude-code`
*   **Authentication:** Run `claude login` to authenticate with your Claude Max subscription.

### Commands

*   **Install Dependencies:**
    ```bash
    pnpm install
    ```

*   **Run Development Server:**
    ```bash
    pnpm tauri dev
    ```
    This starts the Vite dev server (port 1420) and the Tauri application.

*   **Build for Production:**
    ```bash
    pnpm tauri build
    ```

## Key Directories and Files

*   **`src/`**: React frontend source code.
    *   `components/Graph/`: Core graph UI logic (ReactFlow, custom nodes, alignment).
    *   `components/ChatPanel/`: Chat interface and message streaming.
    *   `store/useGraphStore.ts`: Main Zustand store for graph state and persistence.
    *   `lib/tauri.ts`: Wrappers for Tauri commands.
*   **`src-tauri/`**: Rust backend source code.
    *   `src/lib.rs`: The core backend logic. Contains all Tauri commands, ACP client implementation, session management, and permission filtering.
    *   `tauri.conf.json`: Tauri configuration (bundle settings, permissions, window config).
    *   `acp/`: Standalone ACP reference implementation (for testing/debugging).

## Development Conventions

*   **State Management:** Use **Zustand** for global state. The graph state is strictly managed here to ensure synchronization with the ReactFlow canvas.
*   **IPC:** Frontend invokes backend logic via `invoke` (commands) or listens for events (streaming). Backend emits `stream-chunk` events for LLM responses.
*   **Async/Await:** The backend uses `tokio`. Note that ACP client integration requires `tokio::task::LocalSet` due to non-Send futures in the SDK.
*   **Styling:** CSS modules or plain CSS imported in components.
*   **Permissions:** The backend enforces a strict permission model. Read/Search is usually allowed in the notes directory; Write/Edit requires careful handling or user approval (implemented via "Auto-deny" for safety or explicit prompts).
