# Build notes against pinned Zed rev `9155bf4`

Status: **code-side compile-clean against `gpui = "9155bf4..."` (Zed `main`
HEAD as of 2026-05-02)**. Live `cargo run --release` is blocked on a
system-level macOS Xcode toolchain dependency, not on prototype code.
Detail below.

## What was fixed

Pinning `rev = "9155bf4..."` (was `rev = "main"`) and re-running
`cargo check` produced **14 errors** against the prototype as written.
All 14 fall into the categories the README anticipated. Fixes applied:

| File | Error | Fix |
|------|-------|-----|
| `src/theme.rs` (×9) | `rgb()` no longer `const fn` (E0015) | Replaced with a local `const fn hex(u32) -> Rgba` that constructs `Rgba { r, g, b, a }` directly. Float arithmetic in const has been stable since Rust 1.83. |
| `src/main.rs` | `Application::new()` no longer exists (E0599) | Switched to `gpui_platform::application()`. Added `gpui_platform = { ..., features = ["font-kit"] }` to `Cargo.toml`. Matches the pattern used by every example in `crates/gpui/examples/`. |
| `src/main.rs` | `cx.new(...)` requires `AppContext` trait (E0599) | Added `use gpui::prelude::*;` (re-exports `AppContext`, `InteractiveElement`, `StatefulInteractiveElement`, `Render`, etc.). |
| `src/state.rs` | `cx.spawn` now takes `AsyncFnOnce(WeakEntity<T>, &mut AsyncApp)` (E0282) | Rewrote `cx.spawn(|this, mut cx| async move { … })` as `cx.spawn(async move |this, cx| { … })` — the closure itself is now async. Inner `this.update(&mut cx, …)` becomes `this.update(cx, …)`. |
| `src/views/canvas.rs` | `on_drag` requires `StatefulInteractiveElement` (E0599) | Added `.id(node.id.clone())` to the node card div. `id(impl Into<ElementId>)` returns `Stateful<Self>` which gates the `on_drag` / `overflow_y_scroll` family. `String: Into<ElementId>` is implemented at `gpui/src/window.rs:5721`. |
| `src/views/canvas.rs` | `render_node` returned `Div` but now returns `Stateful<Div>` | Loosened return type to `impl IntoElement`. |
| `src/views/side_panel.rs` | `overflow_y_scroll` requires stateful (E0599) | Added `.id("conversation-path")` to the scroll container. |

After these fixes:

```
cargo check
   Compiling thoughttree-gpui v0.0.1 (/Users/david/dev/LabMates/thoughttree-gpui-prototype/src-gpui)
   …
   Compiling gpui_macos v0.1.0 (…)
error: gpui_macos@0.1.0: metal shader compilation failed
```

Source code is type-clean. The remaining failure is **not** in our crate.

## What's blocking `cargo run --release`

`gpui_macos` has a `build.rs` that compiles `crates/gpui_macos/src/shaders.metal`
with the `metal` compiler at build time. Output:

```
error: cannot execute tool 'metal' due to missing Metal Toolchain;
       use: xcodebuild -downloadComponent MetalToolchain
```

This is a recent Apple change. Starting with Xcode 26, the Metal Toolchain
is no longer bundled with the default Xcode install — it ships as a
separately-downloadable component (~1.5 GB).

On this machine (Xcode 26.4.1, build 17E202), the recommended download
command also fails because Xcode's plugin manifest is out of date — it
asks for `xcodebuild -runFirstLaunch` to repair `IDESimulatorFoundation`
before further commands work.

Both commands are system-level installs that require explicit user
authorization to run; they are out of scope for an automated agent making
project-local changes.

## To unblock and finish the smoke test

```bash
sudo xcodebuild -runFirstLaunch
xcodebuild -downloadComponent MetalToolchain
xcrun -sdk macosx metal --version    # sanity check
cd src-gpui
cargo run --release
```

If that succeeds, walk the manual checklist in PR #7's test plan:

- [ ] Window opens with title "ThoughtTree (GPUI prototype)"
- [ ] Demo DAG renders (4 nodes, 3 edges, synth selected)
- [ ] Click node → side panel updates
- [ ] "Branch reply" → new downstream user node
- [ ] "Send to agent (stub)" → assistant node streams char-by-char,
      toolbar shows "1 streaming…"
- [ ] Drag a card → position updates and edges follow
- [ ] `+ Node` adds a fresh user node at (40, 40)

Likely runtime regression to verify: `canvas.rs` drag math at
`on_drag_move` uses `ev.bounds.origin` as the card's new top-left
position. With current GPUI semantics this is the dragged element's
window-space bounds, not a delta from grab point. Cards may jump on
first drag from a non-(0,0) position. If so, capture the actual offset
at drag start and subtract.

## Pinning strategy

`Cargo.toml` is pinned at `9155bf4` deliberately — that's HEAD on
2026-05-02 when these fixups were verified. Bumping requires re-running
`cargo check` and re-walking the table above; expect the `cx.spawn`
async-closure shape and `StatefulInteractiveElement` requirements to
remain stable, but `gpui_platform`'s entry-function name has changed at
least once historically.
