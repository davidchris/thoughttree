## Status

- **Pinned rev:** `6e9ff7a` (zed-industries/zed HEAD as of 2026-07-06).
- **`cargo check`:** clean, 0 errors (8 existing warnings).
- **`cargo build --release`:** clean, 0 errors (8 existing warnings).
- **`cargo test`:** clean, 16 passed.
- **`cargo run --release`:** not re-smoked for the 2026-07-06 bump.

## Bump history

| Pin | Date | Drift fixups | Notes |
|-----|------|--------------|-------|
| `main` → `9155bf4` | 2026-05-02 | 14 errors, all listed under "First-bump fixups" below | Metal Toolchain missing on host; runtime blocked. |
| `9155bf4` → `ae08089` | 2026-05-04 | none | Drift-free bump. Metal Toolchain now present; runtime confirmed. |
| `ae08089` → `6e9ff7a` | 2026-07-06 | 2 fixups, listed under "2026-07-06 bump fixups" below | Verified with offline Cargo cache; host `xcrun metal` again reports missing Metal Toolchain. |

## 2026-07-06 bump fixups

| File | Error | Fix |
|------|-------|-----|
| `Cargo.toml` | `gpui_macos` build script failed compiling shaders: `xcrun -sdk macosx metal` reports missing Metal Toolchain. | Kept the git pin and `font-kit`; added GPUI's `runtime_shaders` feature on `gpui_platform` so Cargo does not require build-time Metal shader compilation. |
| `src/views/toolbar.rs` | `flex_grow()` now requires a `f32` grow factor (E0061). | Changed the spacer to `.flex_grow(1.0)`. |

## First-bump fixups (`rev = "main"` → pinned)

For anyone resurrecting `rev = "main"` from an old checkout, these were the
14 errors that fell out and the patches that resolved them:

| File | Error | Fix |
|------|-------|-----|
| `src/theme.rs` (×9) | `rgb()` no longer `const fn` (E0015) | Local `const fn hex(u32) -> Rgba` constructing `Rgba { r, g, b, a }` directly. Float arithmetic in const has been stable since Rust 1.83. |
| `src/main.rs` | `Application::new()` no longer exists (E0599) | `gpui_platform::application()`. Added `gpui_platform = { ..., features = ["font-kit"] }` to `Cargo.toml`. Matches the examples under `crates/gpui/examples/`. |
| `src/main.rs` | `cx.new(...)` requires `AppContext` trait (E0599) | `use gpui::prelude::*;` re-exports `AppContext`, `InteractiveElement`, `StatefulInteractiveElement`, `Render`. |
| `src/state.rs` | `cx.spawn` now takes `AsyncFnOnce(WeakEntity<T>, &mut AsyncApp)` (E0282) | `cx.spawn(async move |this, cx| { … })` — closure itself is async. Inner `this.update(&mut cx, …)` becomes `this.update(cx, …)`. |
| `src/views/canvas.rs` | `on_drag` requires `StatefulInteractiveElement` (E0599) | `.id(node.id.clone())` on the node card div. `id(impl Into<ElementId>)` returns `Stateful<Self>` which gates `on_drag` / `overflow_y_scroll`. `String: Into<ElementId>` lives at `gpui/src/window.rs:5721`. |
| `src/views/canvas.rs` | `render_node` returned `Div` but now returns `Stateful<Div>` | Return `impl IntoElement`. |
| `src/views/side_panel.rs` | `overflow_y_scroll` requires stateful (E0599) | `.id("conversation-path")` on scroll container. |

## Bumping the pin

```bash
git ls-remote https://github.com/zed-industries/zed HEAD
# update Cargo.toml `rev = "..."` for both gpui and gpui_platform
cd src-gpui
cargo check
cargo build --release
cargo run --release
```

Empirically, bumps within `main` are often drift-free (the `9155bf4` →
`ae08089` bump touched zero source). If `cargo check` does flag drift,
the categories in the first-bump table cover the recurring patterns.

## Manual smoke checklist

- [ ] Window opens with title "ThoughtTree (GPUI prototype)".
- [ ] Demo DAG renders.
- [ ] Click node → side panel updates with conversation path.
- [ ] "Branch reply" → new downstream user node.
- [ ] "Send to agent" → ACP streams real Claude reply into a new
      assistant node; streaming flag clears at end.
- [ ] Drag a card → position updates and edges follow.
- [ ] `+ Node` adds a fresh user node.

## Metal Toolchain (historical)

Earlier bumps on this machine failed in `gpui_macos`'s `build.rs` with
`error: cannot execute tool 'metal' due to missing Metal Toolchain`.
Apple split the Metal Toolchain off from the default Xcode install at
Xcode 26. Resolution path:

```bash
sudo xcodebuild -runFirstLaunch
xcodebuild -downloadComponent MetalToolchain
xcrun -sdk macosx metal --version
```

On 2026-07-06, `xcrun -sdk macosx metal --version` again reported the
missing Metal Toolchain inside the sandbox. The current pin enables
`gpui_platform/runtime_shaders`, which avoids build-time shader compilation.
