# PRD: Reasoning effort per global & project scope

Source: [issue #9](https://github.com/davidchris/thoughttree/issues/9), refined in a grilling session on 2026-07-06. See ADR-0002 (unified static reasoning-effort scale) and CONTEXT.md (Reasoning effort, Provider, Config store).

## Problem Statement

Model and reasoning effort are independent levers, but only the model is configurable. Users who want to trade latency/cost against reasoning depth must switch models instead — there is no way to run the same model harder for a gnarly branch or lighter for quick ones.

## Solution

Add **Reasoning effort** as a per-Provider preference mirroring the existing model-preference pattern end to end: a unified discrete scale `low | medium | high | xhigh`, an effort `<select>` next to each `ModelSelector` in the Settings dialog (global and project sections), project-overrides-global resolution, and spawn-time delivery to each Provider's native mechanism. Providers declare their supported subset statically in the TS descriptor; Gemini declares none and shows no control. Absence of a preference means the Provider's CLI decides (its own config/default) — ThoughtTree stays silent unless the user chose something.

## User Stories

1. As a user, I want to set a default reasoning effort per provider globally, so that every project runs at my preferred depth without per-project setup.
2. As a user, I want to override reasoning effort per project, so that a hard-reasoning project can run `xhigh` while others stay light.
3. As a user, I want effort and model preferences independent, so that a project can override one without clobbering the other (project effort + global model, and vice versa).
4. As a user, I want a "Default" option, so that I can hand control back to the CLI's own configuration (e.g. my `~/.codex/config.toml`).
5. As a user, I want the effort control to show only levels my provider supports, so that I can't pick something meaningless.
6. As a Gemini user, I want no effort control rather than a dead one, so that the UI doesn't promise what the provider can't do.
7. As an existing user, I want my current config and project files to load unchanged, so that upgrading costs nothing.
8. As a user, I want summaries and model discovery to run at minimal effort, so that housekeeping calls stay fast and cheap.
9. As a maintainer, I want effort plumbing to mirror model plumbing shape-for-shape, so that the next preference lever has an obvious template.

## Implementation Decisions

- **Scale (ADR-0002):** unified `low | medium | high | xhigh`, verified against vendor docs. Codex's `minimal` unexposed. `max` (Claude, session-only) unexposed. No numeric budgets.
- **Canonical term:** *Reasoning effort* (CONTEXT.md). Type name both sides: `ReasoningEffort`. Avoid "effort level", "thinking budget".
- **Capability declaration:** `supportedEfforts: ReasoningEffort[]` on the **TS** `ProviderDescriptor` only — `claude-code` and `codex` → all four, `gemini-cli` → `[]` (control not rendered). No Rust capability list; the Rust `ReasoningEffort` serde enum (`#[serde(rename_all = "lowercase")]`) is the validation at the IPC boundary. No model×effort matrix: model-gated `xhigh` (Opus/Sonnet 4.6; only two Codex models) falls back CLI-side.
- **Type shape:** parallel record, not a combined per-provider object: `EffortPreferences = Partial<Record<AgentProvider, ReasoningEffort>>` (TS), `PerProvider<ReasoningEffort>` (Rust). Zero migration; per-lever project/global override falls out of per-key merge.
- **Resolution:** `getEffectiveEffort(provider)` in `useGraphStore`, exact twin of `getEffectiveModel` — project key → global key → `undefined` (CLI default). Frontend resolves; backend receives the resolved value.
- **Transport (spawn-time, per provider):**
  - Claude Code: `.env("CLAUDE_CODE_EFFORT_LEVEL", <effort>)` in `spawn_claude_code_acp` — sidecar's spawned `claude` inherits.
  - Codex: extend `codex_config_args` with `-c model_reasoning_effort=<effort>` alongside the existing `-c model=<id>`.
  - Gemini: never passed; spawn arm ignores effort by construction.
  - Plumbing: `PromptSessionParams.effort: Option<ReasoningEffort>` → `spawn_agent_subprocess(..., effort)`.
- **Summary + model-discovery sessions:** hardcoded `ReasoningEffort::Low` (lowest on offer, universally supported) — not user-configurable.
- **Persistence, global:** Config store key `effort_preferences`; commands `get_effort_preferences` / `set_effort_preference(provider, effort: Option<ReasoningEffort>)` mirroring the model-preference read-merge-write. Missing key → empty record, no migration.
- **Persistence, project:** optional `projectEffortPreferences` on project file V3 (`StoredProviderRecord` style: `null` = explicit clear, stripped on load). **No V3→V4 bump** — additive optional field; accepted consequence: an older app silently drops the field on save.
- **UI:** `<select>` next to each `ModelSelector` in both the "Default Models (Global)" and "Project Models (Override)" sections of `SettingsDialog`; options `Default` (empty value = unset) plus the descriptor's `supportedEfforts`. No control rendered when the list is empty.

## Testing Decisions

- **Rust types seam (existing unit-test module):** `ReasoningEffort` serde round-trip + rejection of unknown values; `PerProvider<ReasoningEffort>` round-trip including `null` entries and unknown provider keys (prior art: model-preference tests).
- **Spawn-args seam (existing `process.rs` test module):** `codex_config_args` emits both `-c` overrides when model+effort set, effort-only, model-only, neither; gemini args unaffected by effort (prior art: `test_gemini_args_default_model_comes_from_descriptor`).
- **Store seam (frontend):** `getEffectiveEffort` resolution order (project > global > undefined); project-file round-trip with `projectEffortPreferences` including `null`-stripping; V3 file without the field loads clean.
- **Settings dialog seam (existing component-test pattern):** effort select rendered for claude-code/codex with `Default` + four options, absent for gemini; change handler writes through the set command / store.
- **Manual acceptance:** live prompt at `xhigh` vs `low` on Claude and Codex, observe thinking-depth difference; summary generation still fast.

## Out of Scope

- Per-generation / per-node effort override and effort provenance on GraphNode — deferred to [backlog/007](./007-per-node-effort-override-and-provenance.md).
- Effort control in the SidePanel `GenerationControls`.
- Gemini thinking support (no CLI surface today; revisit if `gemini` grows one).
- Dynamic effort-capability discovery from the installed CLI (no channel exists; see ADR-0002).
- Exposing Codex `minimal` or Claude `max`.

## Further Notes

Implementation-time verifications flagged during design (none block the design):

- Confirm `CLAUDE_CODE_EFFORT_LEVEL` is honored through the claude-code-acp sidecar's `claude` spawn (env inheritance assumed; verify once live).
- Confirm current codex release accepts `-c model_reasoning_effort=xhigh` at spawn and errors legibly on models that don't support it.
- Changing effort invalidates Claude's prompt cache between sessions — irrelevant today (fresh subprocess per prompt session) but worth remembering if sessions ever become long-lived.
