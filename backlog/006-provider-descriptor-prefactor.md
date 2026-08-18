# Prefactor: provider descriptor table, PerProvider config, generic discovery

Triage: ready-for-agent

## Parent

[backlog/005-codex-provider-prd.md](./005-codex-provider-prd.md) (PRD for [issue #10](https://github.com/davidchris/thoughttree/issues/10))

## What to build

Behavior-neutral refactor that makes adding a Provider a data change instead of a shotgun edit. All per-provider static data moves into one `ProviderDescriptor` table keyed by `AgentProvider` variant; behavior that genuinely differs per provider (spawn shape) stays in a single spawn `match`. Type shape from the design session:

```rust
struct ProviderDescriptor {
    id: &'static str,                                // serde value AND config key
    display_name: &'static str,
    executable_name: &'static str,
    known_paths: &'static [&'static str],
    home_relative_paths: &'static [&'static str],
    env_override: Option<&'static str>,
    install_hint: &'static str,
    version_pattern: &'static str,
    fallback_models: &'static [(&'static str, &'static str)], // (model_id, display_name)
}
```

Included in the slice:

- `AgentProvider::ALL` drives the availability list — no hand-maintained vec.
- `ModelPreferences` and `ProviderPaths` collapse into one generic `PerProvider<T>` (serde-transparent map, `Option<T>` values, String keys). On-disk config JSON stays byte-compatible, including `null` entries and unknown provider keys.
- Per-provider executable finders collapse into one generic discovery: a pure `candidate_paths(descriptor, custom_path)` function (precedence: env override > custom > known > home-relative, nvm iteration included) plus a thin "first path that exists" filesystem check. Known-paths-only security posture preserved (no PATH/`which` lookup).
- Gemini's hardcoded fallback-model special case in model discovery becomes descriptor data (`fallback_models`).
- Frontend: the parallel per-provider name records and settings provider list collapse into one TS descriptor array mirroring the Rust table; `ModelPreferences`/`ProviderPaths` become `Partial<Record<AgentProvider, T | null>>`.

Vocabulary per CONTEXT.md (Provider, ACP adapter); distribution rationale per ADR-0001.

## Acceptance criteria

- [ ] App behaves identically to before: Claude Code and Gemini sessions spawn, stream, and validate exactly as on main
- [ ] Existing `config.json` (with `null` entries) loads without migration; config written by a newer app version containing an unknown provider key still deserializes
- [ ] Descriptor drift guard test: iterating `AgentProvider::ALL`, each variant's serde string equals its descriptor `id`
- [ ] `candidate_paths` precedence covered by unit tests (env override > custom > known > home-relative)
- [ ] Gemini fallback models come from descriptor data; the provider-specific `matches!` special case is gone
- [ ] No hand-maintained provider list remains in the availability command or the frontend settings dialog
- [ ] All existing Rust and frontend tests pass

## Blocked by

None - can start immediately
