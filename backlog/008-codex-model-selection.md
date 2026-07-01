# Codex model selection

Triage: ready-for-agent

## Parent

[backlog/005-codex-provider-prd.md](./005-codex-provider-prd.md) (PRD for [issue #10](https://github.com/davidchris/thoughttree/issues/10))

## What to build

Let users pick a Codex model and have the choice persist, using the existing per-provider machinery end-to-end.

- Populate the Codex descriptor's `fallback_models` with a curated static list — verify exact model ids against the current codex release before shipping (era of `gpt-5.1-codex` / `gpt-5.1-codex-mini`). Model discovery returns empty for Codex, so the fallback list drives the model selector, same mechanism as Gemini.
- Apply the selection at spawn time via the adapter's standard config-override flag: `codex-acp -c model=<id>`. No preference set → spawn without the flag, adapter uses the user's own codex config default. The `set_session_model` path stays untouched.
- Preference persists via the existing `PerProvider` model preferences (global), independent of the Claude and Gemini preferences.

## Acceptance criteria

- [x] Model selector shows the static Codex list when Codex is the active provider
- [x] Selected model is passed to the adapter at spawn (`-c model=<id>`); verified live against codex-acp 0.16.0 — codex session rollout records `"model":"gpt-5.4-mini"` with the flag, `"model":"gpt-5.5"` (user config default) without. Note: codex-acp rejects `session/set_model` with Method-not-found, so the model switch is now gated on the agent advertising model state (`should_set_session_model`).
- [x] No selection → adapter default model, no flag passed
- [x] Preference survives app restart and does not affect Claude/Gemini preferences (existing `PerProvider` config machinery, keyed per provider id)
- [x] Fallback-list resolution covered at the provider-types test seam; model selector behavior covered at the existing component test seam
- [x] Static list ids verified against `@openai/codex` 0.142.5 (latest on npm, 2026-07-02): `gpt-5.5`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.3-codex`

## Blocked by

- [backlog/007-codex-provider-end-to-end.md](./007-codex-provider-end-to-end.md)
