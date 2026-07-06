# Lineage-only graph exposure, injected into the prompt, not served by tools

Agents receive conversation context as a flat message array and have no concept that ThoughtTree is a DAG — a prompt like "synthesize the parent nodes" reads as nonsense to them. We decided to expose graph structure to the agent, bounded by two rules: (1) exposure never exceeds the **Lineage subgraph** (the target node's ancestors plus itself, and the edges among them — never siblings, never the whole graph), and (2) structure is delivered by injecting it into the existing message array at send time (Node markers, Structure annotations, a Lineage map), not by giving the agent graph-query tools.

The scope rule is the load-bearing one. Sibling branches stay invisible even when the user asks to "compare with the other branch": the product's gesture for cross-branch work is creating a Synthesizer node, which makes both branches ancestors and therefore visible. Widening agent visibility past ancestors would duplicate that gesture in a second, invisible mechanism and make what the agent knows unpredictable from the canvas.

The delivery rule follows from the scope rule. Ancestor *content* already ships in the prompt via the Conversation path; only topology metadata is missing, and that is O(path length) small. A `read_node(id)` / `get_graph_structure` tool surface would fetch nothing the prompt doesn't already carry, while costing tool plumbing across three ACP providers, permission-prompt friction, and per-provider tool-support variance. Tools become worth revisiting only if the lineage-only scope is ever loosened — which this ADR says it shouldn't be.

## Considered options

- **Status quo** (structure leaks via natural language only) — rejected: the agent cannot serve the product's core operation, synthesis across branches, and demonstrably misunderstands graph-verb prompts.
- **Whole-graph exposure** — rejected: unbounded token cost and it collapses the point of branching; every branch would contaminate every other.
- **Sibling-widened exposure** (ancestors + siblings of path nodes) — rejected: duplicates the Synthesizer-node gesture, breaks the "context = what the canvas shows converging on this node" invariant.
- **Agent-side graph tools (MCP/ACP)** — rejected for now: redundant under lineage-only scope (see above); ~3× provider integration cost for zero information gain.
- **Intent-gated injection** (send structure only when the prompt mentions nodes/branches) — rejected: brittle NL heuristic; false negatives reproduce the original confusion. Replaced by the **Structure gate**, a purely topological predicate.

## Consequences

- What an agent can know is always readable off the canvas: everything upstream of its node, nothing else. Users reason about context by looking at arrows.
- Cross-branch operations have exactly one path: merge first (Synthesizer node), then ask.
- Linear conversations are byte-identical to today's behavior (the Structure gate ships nothing), so the change is invisible until someone branches.
- Injection composes with provider prompt-prefix caching: annotations and markers are deterministic functions of each node's frozen lineage, and the Lineage map rides on the final user message, keeping earlier messages byte-stable across turns.
- Short node ids become a cross-boundary vocabulary (agent echoes them back), enabling a future agent→UI node-highlighting feature without further protocol change.
