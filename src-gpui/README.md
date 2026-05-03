# ThoughtTree — GPUI front-end prototype

A throwaway exploration of replacing ThoughtTree's React + Tauri front end
with [Zed Industries' GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)
framework. The point isn't to ship — it's to feel out what the same UX looks
like written in idiomatic GPUI so the trade-offs against the React + Tauri
shipping build become concrete.

> Status: **prototype, unverified**. GPUI is a git-only dependency whose
> public API drifts between commits, and it requires Metal (macOS) or Vulkan
> (Linux) at runtime. The code in this directory has not been compiled in CI.
> Treat it as a design sketch you can pull into a clean checkout, then chase
> compile errors against whichever Zed commit you pin.

## Layout

```
src-gpui/
├── Cargo.toml               # gpui = { git = "...", rev = "main" } — pin a commit
├── src/
│   ├── main.rs              # Application::new().run(...) entrypoint
│   ├── app.rs               # AppView — Toolbar | (Canvas + SidePanel)
│   ├── state.rs             # AppState entity (the Zustand-equivalent)
│   ├── theme.rs             # Colors / sizing constants
│   ├── graph/               # Pure-Rust port of src/lib/graph/
│   │   ├── types.rs
│   │   ├── mutations.rs
│   │   └── model.rs         # parents/children/conversation_path (+ test)
│   └── views/
│       ├── canvas.rs        # DAG view — absolute-positioned cards + Bezier edges
│       ├── side_panel.rs    # Conversation-path viewer + branch/send actions
│       └── toolbar.rs       # Project label + "+ Node"
```

The `graph/` module mirrors `src/lib/graph/` exactly — same `GraphNode`,
`GraphEdge`, `Graph`, same conversation-path topo sort. That code is regular
Rust (no GPUI types) and is the part of the prototype most likely to compile
out of the box.

## Domain language

The Rust port keeps the names from `CONTEXT.md`:

| TS                              | Rust (`src-gpui`)                  |
| ------------------------------- | ---------------------------------- |
| `Graph`, `GraphNode`, `GraphEdge` | `graph::{Graph, GraphNode, GraphEdge}` |
| `GraphModel.conversationPath`    | `GraphModel::conversation_path`    |
| `GraphMutations.addNode`         | `GraphMutations::add_node`         |
| Zustand store                    | `Entity<AppState>` observed by views |
| ReactFlow projection             | `CanvasView::render`               |
| ACP session                      | (stub — `AppState::simulate_stream`) |

## What's implemented

- DAG canvas with cards positioned by `Graph.layout`, edges drawn as cubic
  Béziers on a single GPUI `canvas()` element under the cards.
- Selection by click, with the side panel reactively showing the selected
  node's content and its full conversation path.
- "Branch reply" action — creates a downstream user node off the selected one.
- "Send to agent (stub)" — appends a fake reply char-by-char on a background
  task to exercise the streaming render loop without spawning an ACP
  subprocess.
- Drag-to-reposition wired through GPUI's drag API (likeliest part to need
  fixups against current Zed `main`).

## What's deliberately stubbed

- **ACP integration.** `AppState::simulate_stream` fakes the streaming loop.
  The real wiring would call into the existing `src-tauri/src/backend/acp/`
  module, which already has a working `ClientSideConnection` and session
  orchestration. Lifting that into a non-Tauri binary should be mostly a
  matter of replacing Tauri commands with direct method calls — the ACP code
  is already a regular Rust module.
- **Persistence.** No `.thoughttree` save/load. `serde` is in deps so this
  is straightforward; the format on disk is already plain JSON.
- **Markdown rendering.** Side panel shows raw text. GPUI doesn't ship a
  Markdown view; Zed itself uses an internal `markdown` crate that isn't
  exposed.
- **Inline editing.** `editing: Option<NodeId>` is tracked in state but no
  text input is wired up. GPUI's `Editor` lives in another Zed crate; the
  minimum viable shim is `gpui::TextInput` (also internal). For a real
  build you'd port one of those or write a small input element.
- **Auto-layout, recent projects, settings, permissions dialog.** All
  trimmed to keep the prototype scoped to UI-framework comparison.

## How to attempt to build

```bash
cd src-gpui
# Pin a known-good Zed commit before building. `rev = "main"` in Cargo.toml is
# convenient for exploring but won't be reproducible.
cargo run --release
```

Expect compile errors — GPUI's surface area moves quickly. The most likely
classes of breakage:

1. Closure signatures on `cx.observe`, `cx.listener`, `on_mouse_down`,
   `on_drag_move` — recent GPUI threads `&mut Window` through most callbacks.
2. `PathBuilder` constructor / `paint_path` arg order — the path API has
   been reshaped a few times.
3. `WindowOptions` field names (`titlebar` vs `titlebar_options`).
4. `cx.new` vs `cx.new_view` vs `cx.new_model` — these have been unified
   gradually.

When fixing, pin a specific `rev` in Cargo.toml so the prototype keeps building.

## Initial impressions vs React + Tauri

These are the things that became obvious while writing the prototype, not a
verdict — that needs the build to actually run.

**Where GPUI feels lighter than React + Tauri:**

- **One language, one process.** No IPC boundary, no `invoke()`-shaped
  type duplication between TS `types/index.ts` and Rust
  `src-tauri/src/backend/types.rs`. Graph mutations and rendering live in
  the same crate, observing the same `Entity<AppState>`.
- **No bundler, no `bun install`, no Vite dev server.** `cargo run` and
  you're in the app. No CSS pipeline either — styling is method calls on
  the element builder.
- **Streaming updates are trivial.** `simulate_stream` just spawns a
  background task and calls `entity.update()` — no Tauri events, no
  `appendToNode` reducer, no React reconciler heuristics.
- **State sharing without props drilling.** Each view holds an
  `Entity<AppState>` clone and observes it. No context provider tree.

**Where React + Tauri feels lighter than GPUI:**

- **Markdown / rich text rendering is solved on the web side.** Lots of
  ThoughtTree's UI value (`MarkdownContent.tsx`, syntax highlighting,
  copy-to-clipboard on code blocks) comes from web-platform building
  blocks. In GPUI you either embed a webview or rebuild that stack.
- **Distribution.** Tauri produces signed installers; GPUI apps need to
  bring their own bundling story (Zed hand-rolls it).
- **API stability.** Tauri 2.x is stable enough to depend on by version.
  GPUI requires pinning a Zed commit and accepting that updates are an
  ongoing cost.
- **Devtools.** Browser devtools, React DevTools, Zustand inspector —
  none of those exist for GPUI. Debugging is println-style.
- **Talent pool.** The team can ship React. Asking everyone to learn
  GPUI's element builder DSL plus Rust ownership semantics is a
  non-trivial cost for a productivity gain that's mostly invisible to
  users.

**Things that look like a wash:**

- **Performance** for a graph this size. Both ReactFlow and a hand-rolled
  GPUI canvas render thousands of nodes fine. Worth benchmarking before
  using "perf" as a reason to switch.
- **DAG layout.** ReactFlow gives you pan/zoom/selection for free; GPUI
  gives you a blank canvas and `paint_path`. Whichever route you pick,
  most of the work is the same — anchor positions, edge curves, hit
  testing.

## When to revisit this

Only worth taking past prototype if:

1. The team is OK adding Rust + GPUI as a primary front-end skill, and
2. There's a feature that's awkward in the web stack (e.g., shipping
   custom renderers, deep OS integration, CPU-bound layout) that would
   pay back the rewrite, or
3. ThoughtTree starts shipping in a Zed-extension shape where reusing
   GPUI is free.

Otherwise: keep the React + Tauri build, and treat this directory as a
reference for what the alternative would look like.
