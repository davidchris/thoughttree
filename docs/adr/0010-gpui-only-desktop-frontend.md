# ADR 0010: GPUI is the only desktop frontend after v0.6.0

- Status: Accepted
- Date: 2026-10-27

## Context

ThoughtTree has two desktop frontends: the React and Tauri app (`src/`,
`src-tauri/`) and the native GPUI app (`crates/thoughttree-gpui`). The GPUI app
reached parity: all 124 rows in `docs/gpui/parity.md` are verified. The Tauri
app is feature-frozen, but `main` holds unreleased changes since v0.5.0 that
Linux and Windows users would otherwise never get.

Two frontends double the UI work. The Tauri app also renders agent output in a
WebView. The GPUI app is packaged only for macOS. Its Linux execution and
packaging are unverified, and it has no Windows build.

During parity work, the GPUI bundle was `ThoughtTree GPUI.app` with the
identifier `com.david.thoughttree.gpui`, so it could run beside the Tauri app.

## Decision

- Tauri gets one final release, v0.6.0, for macOS, Linux, and Windows. It is
  tagged from `main` with no new fixes. After the release, v0.6.0 gets no
  security or dependency fixes.
- From v0.7.0, releases ship only the GPUI app. The Tauri code then leaves the
  repository.
- The GPUI bundle takes the release name and the Tauri identifier:
  `ThoughtTree.app`, `CFBundleName` `ThoughtTree`, and `CFBundleIdentifier`
  `com.david.thoughttree`. It replaces the Tauri app on macOS, and the GPUI DMG
  replaces the Tauri DMG. Its version comes from the `thoughttree-gpui` crate.
- Linux and Windows stay on v0.6.0 until GPUI packages exist for those
  platforms.
- `packages/graph-model` and the Rust round-trip test against it
  (`crates/thoughttree-gpui-model/tests/parity.rs`) stay while v0.6.0 is the
  Project reader on Linux and Windows.

## Consequences

- macOS users keep their settings and recent Projects when GPUI replaces Tauri.
  Both apps already use `~/Library/Application Support/com.david.thoughttree`.
- A Mac that keeps both apps has two bundles with one identifier, so macOS can
  start the wrong one when it opens the app by identifier. To compare the
  frontends, start each executable directly.
- Linux and Windows users run an unsupported app whose WebView renders agent
  output. The v0.6.0 release notes must say this.
- Projects that GPUI saves must still open in v0.6.0. The round-trip test
  proves this, and a Project format change that v0.6.0 cannot read needs its
  own decision.
- New user-facing features go only into the GPUI app.
