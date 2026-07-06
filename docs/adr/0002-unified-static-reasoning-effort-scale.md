# Unified, statically-declared reasoning-effort scale

Providers expose reasoning effort through incompatible native knobs (Claude Code: `CLAUDE_CODE_EFFORT_LEVEL` env var with `low|medium|high|xhigh`; Codex: `-c model_reasoning_effort` with `minimal|low|medium|high|xhigh`; Gemini CLI: nothing). We decided on one domain-level scale — `low | medium | high | xhigh` — mapped per provider at spawn time, with each provider's supported subset declared **statically** in the TS provider descriptor rather than discovered from the installed CLI.

Static declaration was not the preference — models *are* discovered live from the installed CLI, and we wanted effort to match. But no discovery channel exists: ACP has no effort concept (`NewSessionResponse` carries models and permission modes only), and neither `claude` nor `codex` enumerates its accepted effort values in any machine-readable way. Version-sniffing and probe-spawning were rejected as brittle. When ACP grows an effort concept, swap to discovery the same way models did.

## Considered options

- **Dynamic discovery from installed CLI** — preferred, impossible today (no channel; see above).
- **Per-provider raw knobs** (numeric budget for Claude, native enum for Codex) — max fidelity, rejected: type-union sprawl, per-provider UI widgets, three glossary concepts instead of one. Codex's extra `minimal` stays unexposed.
- **Model×effort capability matrix** (e.g. `xhigh` is model-gated on both vendors) — rejected: maintenance burden tracking two vendors' model lists; unsupported picks fall back CLI-side, harmless.

## Consequences

- The four-value vocabulary is the only cross-language contract: TS descriptor `supportedEfforts` drives the UI; the Rust `ReasoningEffort` serde enum validates at the IPC boundary. No Rust-side capability list (would drift with no consumer).
- Preference values are persisted (config `effort_preferences`, project `projectEffortPreferences`), so changing the scale later means data migration.
- A newly released effort level requires an app update to appear, even if the installed CLI already supports it.
