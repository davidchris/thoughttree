---
status: done
priority: high
tags: [feature, design]
created: 2026-07-02
completed: 2026-07-02
---

# Palette: node search (⌘K)

Implemented via TDD: `src/lib/palette/` (17 tests through `PaletteSearch.search`), `src/components/Palette/` (10 tests), flash styling in `Graph/styles.css`, `flashNodeId`/`settingsOpen` lifted into `useUIStore`, mounted in `App.tsx`.

Post-implementation multi-agent review (15 findings) fixed: matching now runs case-insensitive regex over original strings (`search` returns `{hits, total}` with a limit; no lowercased copies, positions never drift); surrogate-safe truncation; titles skip leading blank lines; palette closes when a permission request/settings dialog appears while open; `open()` reconciles stale `editingNodeId`; IME composition Enter ignored; Tab focus-trapped; overlay closes on mousedown-on-backdrop (not click); active row scrolls into view; ⌘K also matches `e.code === 'KeyK'`; keydown listener attaches once via refs; flash lifecycle owned by `useUIStore.flashNode` (self-expiring, restarts on re-jump); `.streaming.flash` runs both animations; `settingsOpen` survives `reset()`; ContextMenu Escape moved to capture phase.

Source: idea 02 in `docs/improvement-ideas.html`, refined in a grilling session on 2026-07-02. Domain terms (Palette, Corpus snapshot, Search hit, Jump) are in CONTEXT.md.

## Problem

There is no way to search node content. `search_files` only matches filenames for `@`-mentions. Graphs earn their keep at 40+ nodes — exactly where panning and squinting stops working.

## Decisions (grilled, one by one)

1. **Scope:** search-only v1. No commands — but `SearchHit` is a discriminated union (`kind: 'node'`) so a `CommandHit` can join in v2 without rework.
2. **Corpus:** `content` + `summary`, both roles. Summary hits rank above content hits. Images-only user nodes stay findable via summary.
3. **Match semantics:** multi-token AND-substring, case-insensitive. Split query on whitespace; every token must appear in content or summary. No fuzzy-subsequence (noise over prose), no exact-phrase (word order shouldn't matter). Zero deps.
4. **Ranking:** summary-hit > content-hit > earliest match position > recency (`contentUpdatedAt` desc).
5. **Result row:** title (summary, else content head) + one snippet line centered on first match with highlighted tokens (skipped for summary-only matches) + role chip; agent chip shows provider short-name. Render top 20, scroll, show total match count when truncated.
6. **Jump (↵):** set `selectedNodeId` + `setCenter(x, y, { zoom: 1, duration: 400 })` + close + brief flash. ⌘↵ additionally opens side panel preview. Selection composes with existing shortcuts (Space preview, E edit, Enter reply). Flash via transient `flashNodeId` in `useUIStore`, cleared ~1s.
7. **⌘K semantics:** global toggle, works mid-edit in textareas; suppressed while SettingsDialog / SetupWizard / ProjectOpeningWizard / PermissionDialog are open. Escape closes palette only — handle in capture phase + `stopPropagation` (SidePanel/SettingsDialog/ContextMenu also listen for Escape on window). Input autofocused; no focus-restore on close in v1.
8. **Empty query:** ~10 recent nodes by `contentUpdatedAt` desc (pure recency, no match rank). Palette doubles as "recently touched" navigator.
9. **Snapshot, not live:** corpus frozen at palette-open. Live recompute would re-rank rows under the cursor during streaming flushes (100ms). GraphModel mutations are pure, so old node objects never mutate — an array of `GraphNode` refs IS a true snapshot, captured for free. Streaming nodes are findable (content-as-of-open) and jumpable — lineage-blocking blocks editing, not selection.
10. **Node refs, not projections:** `NodeHit.node` is a `GraphNode` ref, not a mapped `PaletteEntry` copy. A projection would be a shallow pass-through failing the deletion test.

## Data structures

```ts
// src/lib/palette/types.ts
import type { GraphNode } from '../graph';

/** Half-open [start, end) char range into the text it accompanies. */
export interface TextSpan { start: number; end: number }

/** Display-ready text with matched ranges. Spans sorted, non-overlapping. */
export interface HighlightedText { text: string; spans: TextSpan[] }

export interface NodeHit {
  kind: 'node';                // discriminant; v2 adds CommandHit
  node: GraphNode;             // ref into snapshot (immutable by construction)
  title: HighlightedText;      // summary, else content head
  snippet?: HighlightedText;   // content line around first match; absent if summary-only match
}

export type SearchHit = NodeHit;  // future: NodeHit | CommandHit
```

## Interface (deep module)

```ts
// src/lib/palette/search.ts — namespace-object pattern, like GraphModel/GraphMutations
export const PaletteSearch = {
  /** query '' → recent nodes (contentUpdatedAt desc, empty nodes excluded).
   *  Returns ALL hits, ranked. Caller slices top-20 + shows hits.length. */
  search(corpus: readonly GraphNode[], query: string): SearchHit[];
};
```

Everything else — tokenization, AND-matching, rank order, snippet windowing, span merging — is implementation, invisible to the component and tested through `search()` only. No `matchedField`, no numeric score in the interface.

## Component

- `src/components/Palette/` mounted in `App.tsx`; overlay above canvas.
- Captures corpus on open: `Array.from(useGraphStore.getState().graph.nodes.values())`, empty nodes filtered.
- Renders `HighlightedText` as React text spans — never `dangerouslySetInnerHTML`.
- Jump reads layout position from the **live** graph at jump time (node may have moved since snapshot).

## Edge cases

- Empty nodes (no content, no summary): excluded at snapshot time.
- Node deleted while palette open: jump target gone → no-op, close palette.
- Verify programmatic `selectedNodeId` set stays in sync with ReactFlow selection state (flagged during grilling, unverified).

## Testing

TDD the pure lib first: rank order, AND-semantics, case-insensitivity, snippet windowing, span correctness, empty-query recency, empty-node exclusion — all via `search(corpus, query)`. Component test follows `SidePanel.test.tsx` pattern: open, type, arrow, ↵, assert selection + close.

## Out of scope (v2+)

- Commands (`CommandHit`, `>` prefix, would fix dead ⌘L binding)
- Selection history / focus restore on close
- Fuzzy matching, match count badges on canvas
