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

**Project file**:
A `.thoughttree` file persisting one Graph (nodes, edges, Layout). The unit of opening, saving, syncing, and conflict.
_Avoid_: document, save file, project (alone — that's the open app state, not the file).

**Vault**:
The user's synced directory where Project files live, shared across devices and writers (desktop, server, torhaus). Sync authority is the file-sync service, never ThoughtTree.
_Avoid_: notes directory (config-key name, not the concept), workspace.

**Guarded write**:
The only legal way to persist a Project file: atomically replace the whole file, conditioned on the content read before mutating still being current. A stale write is rejected; the writer reloads and reapplies. No locks, no partial writes.
_Avoid_: save (unqualified), lock, transaction.

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

**Deployment shape**:
One of three ways the runtime is consumed, identical Graph semantics in all: Shape A — desktop in-process; Shape B — desktop app toggling an embedded server for its own network; Shape C — headless server near the Vault. Clients don't know which shape serves them.
_Avoid_: mode, variant, edition.

**Turn**:
One prompt→response execution within an ACP session, streaming into one GraphNode. Owned by the runtime, not by any client: it runs to completion (or a Parked permission) even if every Attachment drops.
_Avoid_: request, generation, run.

**Turn provenance**:
A best-effort record of the available evidence about a Turn, attached to its assistant GraphNode. Its completeness is explicitly complete, partial, or unknown; optional origin identifiers are descriptive metadata and never establish identity or re-import behavior.
_Avoid_: audit trail, execution log, trace (all imply stronger completeness guarantees).

**Provenance completeness**:
The capture adapter's claim about supported Turn provenance: complete means the full Turn was observed and all supported items retained; partial means a loss is known; unknown means loss cannot be determined. A GraphNode without Turn provenance makes no completeness claim.
_Avoid_: confidence, accuracy, audit completeness.

**Turn reference**:
A canonical persisted record of a URL or file evidenced by a Turn, ordered by first appearance. Relations are additive and observed-only; URL text is preserved exactly and only HTTP(S) is clickable, while Vault files use Vault-relative paths and external files retain only a non-clickable display name.
_Avoid_: link (URLs are only one kind), attachment (a live client subscription), dynamically parsed reference.

**Assistant commentary**:
User-visible progress text emitted by the assistant during a Turn, preserved verbatim and in order as Turn provenance. It excludes hidden reasoning and chain-of-thought and is collapsed by default when displayed.
_Avoid_: reasoning, analysis, summary.

**Turn activity**:
The ordered sequence of Assistant commentary, Tool activity, and Unknown activity observed during a Turn. Persisted sequence order is authoritative; timestamps are optional metadata and never reorder it.
_Avoid_: event log, trace, history.

**Tool activity**:
A logical tool invocation recorded as Turn provenance with a normalized kind: read, edit, delete, move, search, execute, fetch, delegate, or other. It is ordered by first appearance; lifecycle updates refine the same item, unfinished activity becomes incomplete when its Turn closes, and terminal state never regresses.
_Avoid_: tool event, tool log, tool-call update.

**Unknown activity**:
An unrecognized Provider item retained as Turn activity using only its type name, safe display label, and observed order. Its raw payload is discarded and its presence makes Turn provenance partial.
_Avoid_: raw event, unsupported tool.

**Attachment**:
A client's live subscription to a Graph's updates and streams. Many Attachments may watch one Graph; prompting is arbitrated — one active Turn per Graph, further prompts rejected while it runs.
_Avoid_: connection (transport-level), session (taken by ACP session).

**Parked permission**:
A permission request pending while nobody answers it. Pauses its Turn indefinitely — no timeout, no auto-deny; surfaced first when an Attachment (re)appears.
_Avoid_: stale prompt, timed-out request.

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
- A **Vault** contains many **Project files**; each **Project file** persists one **Graph**
- Every persist of a **Project file** is a **Guarded write**, regardless of **Deployment shape** or writer
- A **Deployment shape** determines where **ACP session**s run; **Graph** semantics never vary by shape
- A **Turn** belongs to one **ACP session** and streams into one **GraphNode**; it survives losing all **Attachments**
- **Turn provenance** belongs to its assistant **GraphNode**, is excluded from the **Conversation path**, and is retained when that GraphNode is included in a subgraph export
- A **Parked permission** pauses its **Turn** until answered through an **Attachment** (routed via the **Permission channel**)
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
