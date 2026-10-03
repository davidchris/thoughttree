# Native GPUI desktop frontend

The `thoughttree-gpui` executable uses GPUI 0.2.2 and native GPUI components.
It shares the Rust provider, Vault, attachment, and recovery services with the desktop backend.
The existing Tauri application remains available.

The [parity matrix](parity.md) records the acceptance requirements and their verification status.
Unchecked rows are not verified. Screenshots alone cannot establish full feature parity.
The [capture record](screenshots/README.md) contains the available running-app screenshots and the remaining capture work.

## Build and start on macOS

Install the current stable Rust toolchain and Xcode command-line tools.
GPUI uses the macOS GPU and window system, so the application needs a graphical session.
Build from the repository root:

```sh
cargo run --locked -p thoughttree-gpui
```

Pass a project path to open it at startup:

```sh
cargo run --locked -p thoughttree-gpui -- /absolute/path/project.thoughttree
```

The application uses the existing ThoughtTree configuration directory by default.
Set `THOUGHTTREE_CONFIG_DIR` to use a separate configuration.
Set `THOUGHTTREE_LOCAL_STATE_DIR` to isolate recovery snapshots and other local state.
These overrides are useful when comparing the two frontends.

To create a local macOS application bundle:

```sh
scripts/build-gpui.sh --debug
open 'target/debug/bundle/ThoughtTree GPUI.app'
```

Omit `--debug` for an optimized release build.
Use `--debug --no-build` to package an existing debug executable after `cargo build`.
The script respects `CARGO_TARGET_DIR` and uses an ad-hoc signature for local execution.
It does not create a notarized distribution or installer.

## Provider setup

Use Settings to select the notes directory, default provider, executable paths, models, and reasoning effort.
Providers require their own installed and authenticated command-line tools.
Claude also requires the existing ACP sidecar:

```sh
bun run build:sidecar
scripts/build-gpui.sh --debug --no-build
```

The bundle script copies the current architecture's sidecar when it exists.
It does not download, authenticate, or configure a provider.
Codex uses its configured ACP adapter directly.

## Isolated comparison fixture

The [fixture guide](fixtures/README.md) describes the authored project and its node IDs.
Copy it before interacting with autosave:

```sh
GPUI_DEMO_DIR="$(mktemp -d /tmp/thoughttree-gpui-demo.XXXXXX)"
cp -R docs/gpui/fixtures/. "$GPUI_DEMO_DIR/"
THOUGHTTREE_CONFIG_DIR="$GPUI_DEMO_DIR/config" \
THOUGHTTREE_LOCAL_STATE_DIR="$GPUI_DEMO_DIR/state" \
  target/debug/thoughttree-gpui "$GPUI_DEMO_DIR/parity.thoughttree"
```

Select the copied fixture directory as the notes directory.
Use Reload file after copying if the file card reports a changed modification time.

The optional `scripts/gpui-fixture-agent.py` adapter provides deterministic ACP responses for local verification.
It identifies its responses as fixture data and does not contact a model.
Configure its absolute path as the Codex executable in the isolated Settings.
Set `CODEX_PATH` to the same path when summaries also need the fixture adapter.
Do not present fixture responses as live provider verification.

## Rendering

Markdown, tables, code, links, and selection use native GPUI text elements.
Mermaid uses the Rust `mermaid-rs-renderer` parser and SVG renderer.
TeX uses the bundled MathJax engine in `mathjax-svg-rs`, which produces SVG in process.
GPUI displays those SVGs through its native image renderer.
The interface does not use a browser or WebView.

Copy returns the exact source Markdown. In a focused rich answer, Cmd/Ctrl+A selects the whole answer. Cmd/Ctrl+C copies its source.
Mouse selections copy selected text through the native text component.
This replaces the browser's selection-length heuristic with an explicit whole-answer selection.
Wide code lines wrap inside the panel. Copy preserves their original whitespace and line breaks.
Formulas and diagrams use native image blocks with their source retained in the answer.

Malformed or unsupported diagrams and formulas show a rendering error with the exact source.
This fallback keeps source content available. It does not count as successful diagram or math rendering.
Native tests render 23 Mermaid diagram families, including flowcharts, sequence diagrams, class diagrams, state diagrams, ER diagrams, and charts.
The Rust renderer has its own parser and layout engine. These representative tests do not establish exhaustive Mermaid syntax compatibility.
Remote images use a bounded HTTP client. Answer text cannot load arbitrary local files.
SVGs cannot load external resources or execute scripts. Bounded embedded raster images support C4 person symbols.

## Linux baseline

Linux CI checks the native frontend with Clippy. Linux desktop execution and packaging remain unverified.
A Vulkan-capable graphics driver and a graphical X11 or Wayland session are required.
The initial Debian or Ubuntu dependency baseline is:

```sh
sudo apt-get install build-essential clang cmake pkg-config libfontconfig-dev \
  libglib2.0-dev libssl-dev libwayland-dev libx11-xcb-dev \
  libxkbcommon-x11-dev libvulkan1 libzstd-dev xdg-desktop-portal
```

This baseline follows the relevant packages in [Zed's Linux setup script](https://github.com/zed-industries/zed/blob/main/script/linux).
Distribution versions can require additional packages. No Linux packaging claim is made here.
On Linux, `scripts/build-gpui.sh` builds the executable and copies an existing matching sidecar beside it.

## Checks and evidence

Local macOS verification on 2026-10-03 passed:

- Full Rust workspace: 346 tests passed, with one existing ignored core test.
- Native GPUI: 111 tests passed, included in the workspace total.
- Existing TypeScript frontend: 345 tests passed across 28 files.
- Workspace formatting and Clippy with warnings denied.
- TypeScript type checking, ESLint with no warnings, and the core/Tauri dependency boundary check.
- Locked offline native executable build and local macOS debug bundle creation.
- Strict bundle signature verification and property-list validation.

At [commit 6ff1b32](https://github.com/davidchris/thoughttree/commit/6ff1b320b393f149b2658d29d0ebcb1366b2ce12), Linux CI, GPUI macOS CI, frontend checks, and CodeQL passed.

The [capture record](screenshots/README.md) contains ten paired states from the running applications.
The [matrix](parity.md) verifies 121 of 123 requirements. Real OS file and image drops remain unverified.

Run focused native checks first:

```sh
cargo test -p thoughttree-gpui-model -p thoughttree-desktop -p thoughttree-gpui
cargo clippy -p thoughttree-gpui --all-targets -- -D warnings
```

Before submitting the change, run the repository checks from `CLAUDE.md`, including the full workspace suite.
The HTTP client integration test uses a local TCP listener and needs an environment that permits loopback sockets.
Native interaction tests use GPUI's test platform. Screenshots need a graphical session.

Store screenshots from the running applications with their fixture, window size, action sequence, and observed result.
The matrix separates visual evidence from behavior such as save conflicts, late events, and permission response routing.
