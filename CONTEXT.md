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

**Layout**:
Per-node spatial position `{x, y}` used to render the Graph in ReactFlow. Stored inside Graph (persisted with project file) but semantically separate from node content.
_Avoid_: position (ambiguous), coords.

**ReactFlow projection**:
The transformation `(Graph, uiState) → ReactFlow Node[]` that produces presentation nodes for `@xyflow/react`. ReactFlow `node.data` carries only `{ id }`; node components subscribe to the store by id for content.
_Avoid_: node mapping, view model.

**ACP session**:
A single subprocess instance of `claude-code-acp` (or sibling provider CLI) that the Rust backend drives via the Agent Client Protocol. Owns one streaming conversation.
_Avoid_: agent, worker.

**Provider**:
A backend LLM source (e.g., `claude-code`, `gemini-cli`). Each Provider has discoverable executable paths and a list of available models.
_Avoid_: backend, vendor.

## Relationships

- A **Graph** contains many **GraphNodes** and many **GraphEdges**
- A **GraphNode** has zero, one, or many parent **GraphEdges** — multiple parents = **Synthesizer node**
- A **Conversation path** is derived from a **Graph** and a target **GraphNode**
- The **GraphModel** operates on a **Graph**; the Zustand store holds a **Graph** value and calls **GraphModel** for mutations
- The **ReactFlow projection** consumes a **Graph** plus UI state; ReactFlow itself never sees **GraphNode** directly
- An **ACP session** is spawned per **Provider** and bound to one streaming **GraphNode** at a time

## Example dialogue

> **Dev:** "When a user creates a node downstream of two existing assistant nodes, what does the **Conversation path** look like?"
> **Domain expert:** "It's a **Synthesizer node**. The path includes all ancestors topo-sorted by timestamp. If two consecutive ancestors are both `assistant`, we merge their content into one message before sending — Claude expects role alternation."

> **Dev:** "Should the **GraphModel** know about ReactFlow?"
> **Domain expert:** "No. The **ReactFlow projection** is the only place `@xyflow/react` types appear on the read path. **GraphModel** is pure TS — testable without rendering."

## Flagged ambiguities

- "node" was used to mean both **GraphNode** (domain) and ReactFlow `Node<ThoughtTreeFlowNodeData>` (projection) — resolved: the latter is the **ReactFlow projection**'s output, never called just "node" in domain code.
- "lineage" was used to mean both ancestor *set* and ordered *path* — resolved: lineage = set, **Conversation path** = ordered.
