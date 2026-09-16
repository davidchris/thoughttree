# Changelog

## Unreleased

- Fixed Codex startup from macOS desktop launches where Node was missing from PATH.
- Fixed ignored model and reasoning effort choices with current Codex ACP adapters.
- Codex model menus now use live discovery, with current Astra, Sol, Terra, Luna, GPT-5.5, and Spark fallbacks.
- Codex is the default provider for new configurations. Automatic node headings now use the default provider.
- Removed Gemini execution, installation settings, and provider choices. Saved messages remain readable.
- Assistant replies now separate progress commentary from the final answer with a horizontal rule when tool calls or thinking split the message. Applies to Codex and Claude Code.
- Updated the ACP client library to 2.x. Codex turns no longer log `failed to decode ... usage_update` errors, and context usage is logged per turn. Model discovery and switching work on both the config-option API (Codex) and the legacy `models` API (Claude Code sidecar).

- Guarded saves now serialize local writers across revision validation and atomic replacement to prevent lost updates.
- Building the core crate now requires Rust 1.89 or later.

### Added

- Turn provenance for live replies - Every assistant reply now records which files the agent read, edited, searched, or otherwise touched, plus the tool activity of the Turn. Open the reply in the side panel and expand "Provenance" to see them. Files inside the notes directory show their Vault path; files outside it show only a name. A cancelled or failed Turn still records what happened before it stopped.
- File nodes - Link a file from your notes directory into the graph by dropping it on the canvas or with right-click → "Add file…". The card shows a type badge, name, size, and a preview (image thumbnail or text excerpt). Connect it to a user node and the file becomes context for everything downstream.
- Images (PNG, JPG, GIF, WebP) are sent inline, read fresh from disk at send time. Limits: 5 MB per image, 8000 px on the longest side, 20 images per prompt; a larger image is refused before any request is made. Every other file type is sent as a pointer the agent reads from disk itself, with no size limit.
- A "changed on disk" badge appears when the file differs from what the node last saw; Refresh adopts the current version. A missing file blocks sending on that branch.
- Project file format v5 - Projects are now saved as v5. Older ThoughtTree builds refuse v5 files so linked files cannot be dropped silently; v3 and v4 files still open and migrate. See `docs/adr/0008-project-file-v5-for-file-nodes.md`.

## 0.5.0

- New Look - Dark teal-black theme with a single mint accent across the canvas, side panel, dialogs, and palette. Tokens live in `design/tokens.css`
- Role by Shape - User nodes show a filled mint dot, assistant nodes an outlined one. Streaming pulses the dot; nodes waiting on an ancestor dim
- Landscape Nodes - Canvas nodes are now 170×120 (1:√2, like DIN A paper) instead of squares. Tidy graph spacing adjusted to match
- Icon Toolbar - Toolbar buttons carry icons and collapse to icons only in narrow windows, so actions no longer overlap
- Cmd+, Settings - Opens Settings with the standard macOS shortcut
- New App Icon - Mint tree on teal-black; reduced mark used for the favicon and toolbar
- Local Project writes share an advisory lock outside the Vault and recheck revisions before atomic replacement. External writers can bypass this protection.
- Conflicts offer comparison, reload with a recovery snapshot, or a separate copy. Existing Projects cannot be overwritten without their loaded revision.
- Independent recovery snapshots preserve completed ThoughtTree edits. The toolbar and opening screen can reopen snapshots as unsaved Projects.
- Core Vault reads and writes reject file symlinks that escape the root. Desktop relative paths retain notes-directory boundary checks.
- Desktop and local Vault writes run on a blocking pool.

## 0.4.1

- Security - Restored the notes-directory boundary for project load and save commands
- Downloads - Changed the README links to the canonical GitHub repository and release page

## 0.4.0

- Codex Support - Added Codex as a second agent provider alongside Claude Code, with model selection when starting a session
- Cmd+K Node Search - Jump straight to any node in large graphs with a searchable command palette

## 0.3.3

- Faster Streaming - Agent responses now batch rapid token updates, keeping the canvas more responsive during long generations
- ACP Stability - Claude Code sessions now manage subprocess lifecycle more reliably and time out stalled initialization instead of hanging indefinitely
- Safer Summary Generation - Background summaries now keep stricter read-only tool permissions
- Dependency Updates - Updated Tauri and related dependencies with current security fixes
- Build Reliability - CI now checks TypeScript, Rust formatting, Clippy, and backend tests before changes can land

## 0.3.2

- Multi-Parent Context Fix - Synthesizer nodes (multiple parents) now include all parent threads in conversation context; previously extra parent edges were silently dropped
- Agent Node Side Panel - Double-click and `E` shortcut on agent nodes now open the side panel
- @-Mention Positioning - File autocomplete now appears at the cursor instead of the bottom of the textarea
- Summary Hardening - More robust summary generation and graph state/generation flows
- Project File Format v3 - Save format bumped to v3; existing v2 files migrate automatically on load
- Internal Refactor - Graph state rewritten on a functional GraphModel + ReactFlow projection layer (no user-visible change)
- Security - Patched cargo `time` advisory, cleared bun audit findings, added 7-day release-age gate on dependency bumps
- Build - Synced `@tauri-apps/api` and `@tauri-apps/cli` to 2.10 to match Rust `tauri` crate (fixes release CI mismatch)

## 0.3.0

- Image Support - Paste or drag-drop images into conversations, with automatic resizing for API limits
- Side Panel File Mentions - @ autocomplete for file references now available in side panel
- Fixed Node Sizing - Nodes are always 120x120 with corrected hit-boxes for panning
- Side Panel Editing - Double-click on nodes opens side panel instead of inline editing
- CI/CD Pipelines - GitHub Actions workflows for multi-platform release builds and PR tests
- Build Optimizations - Cargo release profile with LTO, symbol stripping, and cross-platform sidecar builds

## 0.2.2

- Parallel Streaming - Multiple simultaneous generations with lineage-based blocking
- Edit Shortcut - Press E to enter edit mode when previewing a user node

## 0.2.1

- Security Hardening - CSP enabled, path validation, traversal protection
- Preview Toggle - Shortcut changed from P to spacebar

## 0.2.0

- Copy as Markdown - Copy button for side panel content
- Generate from Side Panel - Generate responses directly while editing in side panel

## 0.1.0

- External Links - Open hyperlinks from agent responses in default browser
- Date-Aware Agent - Current date injected into prompts (updates on regeneration)
- Canvas Panning - Improved panning UX with spacebar-to-pan support
- LaTeX Rendering - Mathematical expression support in side panel
- Project Wizard - Cmd-O shortcut with recent projects for quick selection
- Auto-Layout - Button to automatically align and structure nodes
- Node Snapping - New nodes align and snap to originating node; drag to reposition
- Mermaid Diagrams - Render diagrams with hidden system prompt for Claude
- Sticky Note Nodes - Square, editable nodes with side panel expansion
- Collapsible Nodes - Collapse to preview mode showing first few words
- Reply from Node - Context menu to create follow-up messages from any node
- Side Panel Preview - Overlay view for scrolling long agent responses
- File Mentions - @ autocomplete to reference files
- Auto-Resize Input - Text box grows while typing
- Sharp Text Zoom - Fixed pixelation when zoomed in
- Markdown in Nodes - Render markdown content in conversation nodes
