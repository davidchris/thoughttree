# Per-node reasoning-effort override + effort provenance on GraphNode

Triage: icebox (deferred from issue #9 design session, 2026-07-06)

## Parent

[Issue #9](https://github.com/davidchris/thoughttree/issues/9) — Settings-scope reasoning effort. This item captures what was explicitly deferred there.

## What to build (later)

1. **Per-generation / per-node effort override**: let the user pick a Reasoning effort when generating a node (SidePanel `GenerationControls`, next to the existing per-generation `ModelSelector`), overriding the project/global settings resolution for that generation only.
2. **Effort provenance**: record the Reasoning effort that produced an assistant response on the **GraphNode** (alongside the existing `provider` + `model` fields), including Graph serialization and any node-UI display.

These two belong together: the override work must touch the node schema anyway, so provenance rides along.

## Context from the issue-#9 design session

- Reasoning effort is a unified discrete scale `low | medium | high | xhigh` (see CONTEXT.md), stored per Provider at global (`config.json` `effort_preferences`) and project (`projectEffortPreferences`) scope.
- Resolution mirrors model: project → global → CLI default. A per-generation pick would sit in front of that chain, exactly like the per-generation model pick does today.

## Blocked by

Issue #9 implementation (settings-scope effort plumbing must exist first).
