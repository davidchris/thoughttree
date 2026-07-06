# ThoughtTree

DAG-based conversation tool. Users branch and merge LLM conversations as a graph rather than a linear chat. Each node is one message; multi-parent nodes synthesize content from converging branches.

## Language

**Graph**:
The conversation DAG. Owns nodes, edges, and per-node layout positions.
_Avoid_: tree (the structure is a DAG, not a tree), canvas, board.

**GraphNode**:
A single message in the Graph — either a user prompt or an assistant response. Pure domain shape; carries content, role, timestamp, and role-specific fields (provider/model for assistant, images for user).
_Avoid_: message, item, card, ThoughtTreeFlowNodeData (that's the ReactFlow projection).

**GraphEdge**:
A directed link from one GraphNode to another, indicating message ordering and lineage. Multiple incoming edges = synthesizer node.
_Avoid_: connection, link, relation.

**GraphModel**:
The functional API over a Graph value. All mutations are pure: `(Graph, args) → Graph`. Owns traversal, lineage, and serialization. Lives in `src/lib/graph/`.
_Avoid_: GraphService, GraphManager, GraphStore (the Zustand store *uses* GraphModel, it isn't one).

**Synthesizer node**:
A GraphNode with multiple parents. Receives content from converging branches as prior conversation turns.
_Avoid_: merge node, join node.

**Conversation path**:
The ordered sequence of GraphNodes used as LLM context for a target node. For multi-parent targets, all ancestors are included, topologically sorted by `timestamp`, with consecutive same-role messages merged (concat content).
_Avoid_: history, thread, lineage (lineage means ancestor *set*, not *ordered path*).

**Lineage subgraph**:
The sub-DAG induced by a target GraphNode's ancestors plus the target itself — those nodes and the edges among them. The hard boundary of what an agent may ever see about the Graph; siblings and other non-ancestors are never exposed. The Conversation path is its topological linearization.
_Avoid_: subtree (it's a DAG), context graph, whole graph.

**Node marker**:
An inline tag wrapping one GraphNode's content inside a Conversation path message, binding that text to its GraphNode id for the agent. Survives same-role merging, so merged messages stay attributable node-by-node.
_Avoid_: label, tag (too generic).

**Structure annotation**:
A one-line in-flow note placed before a user GraphNode's text where the Lineage subgraph has a topology event (fork or merge), describing that event in place. Invisible in the UI; exists only in agent context.
_Avoid_: preamble, header.

**Lineage map**:
A compact adjacency summary of the Lineage subgraph — one line per node: short id, role, parent ids — attached once to the final user message. Topology only; never repeats node content.
_Avoid_: graph dump, structure block, preamble.

**Structure gate**:
The rule deciding whether Node markers, Structure annotations, and the Lineage map ship at all: only when the Lineage subgraph is non-linear (some node has more than one parent or more than one child within it). A purely topological predicate — never inferred from prompt wording. Linear conversations pay zero overhead.
_Avoid_: intent detection, heuristic.

**Layout**:
Per-node spatial position `{x, y}` used to render the Graph in ReactFlow. Stored inside Graph (persisted with project file) but semantically separate from node content.
_Avoid_: position (ambiguous), coords.

**ReactFlow projection**:
The transformation `(Graph, uiState) → ReactFlow Node[]` that produces presentation nodes for `@xyflow/react`. ReactFlow `node.data` carries only `{ id }`; node components subscribe to the store by id for content.
_Avoid_: node mapping, view model.

**Palette**:
The overlay for finding and jumping to GraphNodes. Searches a Corpus snapshot taken when it opens; an empty query lists recently updated GraphNodes.
_Avoid_: quick switcher, omnibar, spotlight, command palette (no commands exist yet).

**Corpus snapshot**:
The frozen collection of GraphNodes the Palette searches, captured at open. Stable for the Palette's lifetime — results never re-rank under the user while the Graph changes (e.g., during streaming). Immutability of GraphNodes makes capture cheap.
_Avoid_: index, cache.

**Search hit**:
One Palette result: a reference to a GraphNode plus display-ready matched-text ranges (title, optional snippet). Discriminated by kind so future non-node results can join.
_Avoid_: result (vague), match (verb/noun confusion).

**Jump**:
The Palette's selection action: select the GraphNode, center the viewport on it, flash it briefly. Composes with existing selection-based shortcuts (preview, edit, reply).
_Avoid_: navigate, go-to, focus (collides with DOM focus).

**ACP session**:
A single subprocess instance of a Provider's ACP adapter that the Rust backend drives via the Agent Client Protocol. Owns one streaming conversation. Orchestrated by `run_prompt_session` / `run_summary_session` / `run_model_discovery_session` in `src-tauri/src/backend/acp/sessions.rs`, driven by an ACP client.
_Avoid_: agent, worker.

**ACP client**:
A `Client` trait impl that receives notifications from the ACP subprocess — `StreamingClient` (chat), `SummaryClient` (summary), `ModelDiscoveryClient` (model list). Lives in `src-tauri/src/backend/acp/clients.rs`. Distinct from ACP session, which is the orchestration around it.
_Avoid_: listener, callback.

**Provider**:
A backend LLM source (e.g., `claude-code`, `gemini-cli`, `codex`). Each Provider has one ACP adapter, discoverable executable paths, and a list of available models.
_Avoid_: backend, vendor.

**ACP adapter**:
The executable a Provider spawns to speak ACP over stdio. Three shapes exist: bundled sidecar wrapping a vendor CLI (`claude-code-acp`), vendor CLI with native ACP flag (`gemini --experimental-acp`), user-installed bridge binary (`codex-acp`). The Provider abstraction hides which shape is in use.
_Avoid_: sidecar (that's one distribution shape, not the concept), agent binary.

**Backend module tree**:
The concern-grouped Rust modules under `src-tauri/src/backend/`: `types`, `state`, `runtime`, `config`, `acp/{clients,process,sessions}`, `commands/{chat,projects,providers,summary}`. `lib.rs` is a thin entry point that registers Tauri commands; all logic lives under `backend/`.
_Avoid_: "the Rust side" as a catch-all when a specific module fits.

**Tauri command**:
A `#[tauri::command]` async fn registered with `tauri::generate_handler!`, invoked from the frontend via `invoke()`. The IPC surface between React and Rust. Lives in `src-tauri/src/backend/commands/`.
_Avoid_: handler, endpoint, RPC.

**Permission channel**:
A `oneshot::Sender<String>` stored in `AppState::pending_permissions` keyed by request id. The ACP client awaits the receiver while the frontend shows a permission prompt; the `respond_to_permission` Tauri command resolves the sender with the user-selected option id.
_Avoid_: callback, promise.

**LocalSet runtime**:
The `run_localset_blocking` helper in `src-tauri/src/backend/runtime.rs`. Spawns a current-thread Tokio runtime + `LocalSet` on a blocking pool, used because ACP futures are `?Send`.
_Avoid_: worker thread, executor.

**Reasoning effort**:
How hard a Provider's model thinks before answering: a single discrete scale `low | medium | high | xhigh`, uniform across Providers. Each Provider maps the scale to its native mechanism, and may support only a subset (or none — Gemini today). Configured per Provider at global and project scope, like model preferences.
_Avoid_: thinking budget (numeric, provider-internal), thinking mode, effort level (redundant — "effort" suffices).

**Config store**:
The `tauri_plugin_store` instance keyed `config.json`, holding notes directory, default provider, model preferences, provider paths, and recent projects. Wrapped by `src-tauri/src/backend/config.rs`.
_Avoid_: settings, preferences (use these for user-facing concepts, not the persisted store).

## Relationships

- A **Graph** contains many **GraphNodes** and many **GraphEdges**
- A **GraphNode** has zero, one, or many parent **GraphEdges** — multiple parents = **Synthesizer node**
- A **Conversation path** is derived from a **Graph** and a target **GraphNode**
- A **Lineage subgraph** is derived from a **Graph** and a target **GraphNode**; the **Conversation path** is its linearization
- **Node marker**s bind **Conversation path** content to **GraphNode**s; **Structure annotation**s and the **Lineage map** describe the **Lineage subgraph**'s topology; all three are controlled by the **Structure gate**
- Cross-branch operations (contrast, synthesis across branches) require a **Synthesizer node** — the agent never sees non-ancestors, so merging branches is the only way to bring them into scope
- The **GraphModel** operates on a **Graph**; the Zustand store holds a **Graph** value and calls **GraphModel** for mutations
- The **ReactFlow projection** consumes a **Graph** plus UI state; ReactFlow itself never sees **GraphNode** directly
- The **Palette** searches a **Corpus snapshot** of the **Graph**'s **GraphNodes**; each **Search hit** references one **GraphNode**
- A **Jump** sets selection and viewport to one **GraphNode**
- An **ACP session** is spawned per **Provider** and bound to one streaming **GraphNode** at a time
- An **ACP session** drives an **ACP client**; user-permission prompts during the session use a **Permission channel** routed back through a **Tauri command**
- All **ACP session**s and model-discovery runs execute on a **LocalSet runtime**
- The **Config store** persists **Provider** paths, model preferences, **Reasoning effort** preferences, default **Provider**, recent project files, and the notes directory
- A **Reasoning effort** preference is resolved per **Provider** — project scope overrides global scope, absence means the Provider's CLI default

## Example dialogue

> **Dev:** "When a user creates a node downstream of two existing assistant nodes, what does the **Conversation path** look like?"
> **Domain expert:** "It's a **Synthesizer node**. The path includes all ancestors topo-sorted by timestamp. If two consecutive ancestors are both `assistant`, we merge their content into one message before sending — Claude expects role alternation."

> **Dev:** "Should the **GraphModel** know about ReactFlow?"
> **Domain expert:** "No. The **ReactFlow projection** is the only place `@xyflow/react` types appear on the read path. **GraphModel** is pure TS — testable without rendering."

## Flagged ambiguities

- "node" was used to mean both **GraphNode** (domain) and ReactFlow `Node<ThoughtTreeFlowNodeData>` (projection) — resolved: the latter is the **ReactFlow projection**'s output, never called just "node" in domain code.
- "lineage" was used to mean both ancestor *set* and ordered *path* — resolved: lineage = set, **Conversation path** = ordered.
