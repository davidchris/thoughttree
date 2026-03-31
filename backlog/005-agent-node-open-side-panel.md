---
status: done
priority: high
tags: [bug]
created: 2026-03-31
completed: 2026-03-31
---

# Agent node: double-click and "E" don't open side panel

Double-clicking on an agent node doesn't open the side panel, and pressing "E" doesn't either. Only clicking the expand triangle (▼) works. This is confusing because double-click works on user nodes.

## Fix

- Added `onDoubleClick` handler to `AgentNode` that calls `setPreviewNode(id)` to open the side panel in preview mode (no edit mode, since agent nodes are read-only).
- Extended the "E" keyboard shortcut to open the side panel for any selected node when the panel isn't already open. When the panel is already open on a user node, "E" still enters edit mode as before.
