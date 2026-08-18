# PRD: Add OpenAI Codex as Provider (ACP)

Source: [issue #10](https://github.com/davidchris/thoughttree/issues/10), refined in a grilling session on 2026-07-01. See ADR-0001 (user-installed ACP adapters) and CONTEXT.md (Provider, ACP adapter).

Vertical slices: [006 prefactor](./006-provider-descriptor-prefactor.md) → [007 Codex end-to-end](./007-codex-provider-end-to-end.md) → [008 model selection](./008-codex-model-selection.md).

## Problem Statement

ThoughtTree users can only drive conversations with Claude Code or Gemini CLI. Users who pay for a ChatGPT subscription have no way to use it in ThoughtTree, even though Codex is ACP-capable via an adapter — they must maintain a second subscription or API key to use the app at all.

## Solution

Add Codex as a third Provider. Users install the `codex-acp` adapter (`npm i -g @zed-industries/codex-acp`) and authenticate once via the Codex CLI's own login (`codex login`, ChatGPT subscription — no key stored in ThoughtTree). Codex then appears in the provider dropdown like any other Provider: availability detection with an install hint when missing, model selection from a curated list, per-project and global model preference persistence, and streaming responses into GraphNodes over an ACP session.

Alongside the feature, the provider abstraction is refactored so that Provider-specific data (executable names, install paths, install hints, fallback models, display names) lives in one static descriptor table — adding a fourth Provider later means adding one enum variant and one descriptor entry, not editing a dozen match arms.

## User Stories

1. As a ChatGPT subscriber, I want to select Codex as my provider, so that I can use my existing subscription in ThoughtTree without a separate API key.
2. As a user, I want Codex to appear in the provider dropdown, so that I can discover the option without reading documentation.
3. As a user without the adapter installed, I want the Codex option visible but disabled with an install hint, so that I know the feature exists and how to enable it.
4. As a user, I want the install hint to cover both the adapter install and the login step, so that I don't end up installed-but-unauthenticated with no guidance.
5. As a user, I want to start a session with Codex and see the response stream into the graph node, so that Codex conversations feel identical to Claude and Gemini ones.
6. As a user, I want to branch and merge Codex conversations in the Graph like any other provider, so that provider choice doesn't limit the core DAG workflow.
7. As a user, I want to pick a Codex model from a list, so that I can trade capability against speed/cost.
8. As a user, I want my Codex model preference to persist globally, so that I don't re-select it every launch.
9. As a user, I want my model preference remembered per provider independently, so that switching providers doesn't clobber my other choices.
10. As a user with a non-standard install location, I want to configure a custom path to the codex-acp executable in Settings, so that ThoughtTree finds my install.
11. As a user configuring a custom path, I want the executable validated before it's saved, so that a typo doesn't silently break sessions.
12. As a user who is not logged in to Codex, I want a clear error when a session starts, so that I know to run `codex login` rather than suspecting ThoughtTree is broken.
13. As a user, I want Codex availability re-checked when I open settings or the provider list, so that installing the adapter takes effect without restarting the app.
14. As a security-conscious user, I want ThoughtTree to only launch executables from known install locations or my explicit configuration, so that PATH injection can't execute arbitrary binaries.
15. As an existing user, I want my current config file to keep working after the update, so that upgrading doesn't reset my provider paths or model preferences.
16. As an existing user on an older app version, I want a config file written by a newer version (containing a provider my version doesn't know) to still load, so that downgrades and shared configs don't crash the app.
17. As a maintainer, I want all per-provider static data in one descriptor table, so that adding a provider is a data change, not a shotgun edit.
18. As a maintainer, I want a test guarding descriptor ids against serde drift, so that a renamed variant can't silently desynchronize config keys.
19. As a maintainer, I want the Gemini fallback-model special case generalized, so that any provider without ACP model discovery declares its list as data.

## Implementation Decisions

- **Adapter distribution (ADR-0001):** Codex integrates via the user-installed `@zed-industries/codex-acp` bridge binary. The Codex CLI itself does not speak ACP (the issue's original premise was wrong); the vendor CLI is only needed for `codex login`. No bundling, no runtime `npx`.
- **Scope split:** this PRD covers the code-pattern unification (descriptor table) plus the Codex Provider. Migrating Claude Code off its bundled sidecar to the same distribution model is a separate follow-up issue (breaking change).
- **Descriptor table:** a static `ProviderDescriptor` per `AgentProvider` variant carries id (serde/config key), display name, executable name, known absolute paths, home-relative paths, optional env-var override, install hint, `--version` output pattern, and fallback model list. Behavior that genuinely differs per provider (spawn shape: sidecar+env vs native ACP flag vs plain adapter) stays in a single spawn `match` — not encoded as descriptor flags. Type shape from the design session:

  ```rust
  struct ProviderDescriptor {
      id: &'static str,                                // "codex" — serde value AND config key
      display_name: &'static str,
      executable_name: &'static str,                   // "codex-acp"
      known_paths: &'static [&'static str],
      home_relative_paths: &'static [&'static str],
      env_override: Option<&'static str>,
      install_hint: &'static str,
      version_pattern: &'static str,
      fallback_models: &'static [(&'static str, &'static str)], // (model_id, display_name)
  }
  ```

  `executable_name` means "the one user-pointable executable": the vendor CLI for Claude Code (sidecar stays bundled for now), the ACP adapter itself for Codex.
- **Config shape:** `ModelPreferences` and `ProviderPaths` collapse into one generic `PerProvider<T>` — a serde-transparent map keyed by descriptor id with `Option<T>` values. On-disk JSON stays byte-compatible with the current per-field structs (including `null` entries). String keys (not enum keys) so configs containing unknown providers still deserialize. Frontend mirrors as `Partial<Record<AgentProvider, T | null>>`.
- **Executable discovery:** one generic finder replaces the per-provider finders, walking env override → custom path → known paths → home-relative paths (nvm iteration included). The known-paths-only security posture (no PATH/`which` lookup) is preserved.
- **Model list:** codex-acp does not populate `available_models` on session creation (it exposes models via ACP session config options instead). Decision: do NOT implement config-options support now. Model discovery returns empty for Codex and falls back to a static list declared in the descriptor; the existing Gemini `matches!` special case is replaced by this data-driven fallback. Teaching ThoughtTree ACP config options is a possible future enhancement.
- **Model selection:** applied at spawn time via `codex-acp -c model=<id>` (codex's standard config-override flag), mirroring Gemini's spawn-time `--model`. No preference set → adapter uses the user's own codex config default. The `set_session_model` path is untouched.
- **Auth:** availability = adapter executable found, nothing more. No pre-flight check of codex auth state (no coupling to `~/.codex/auth.json`). Auth failures surface as session-start errors in the chat panel. Install hint text covers both `npm i -g @zed-industries/codex-acp` and login via `npm i -g @openai/codex && codex login`.
- **Frontend:** provider dropdown is already driven by the backend's availability response, so Codex appears without selector changes. Frontend touches: the `AgentProvider` union, per-provider name records (collapse into one TS descriptor array mirroring the Rust table), the settings dialog's provider list, and the `PerProvider` types.
- **Naming:** serde id `codex`, display name "Codex", short name "Codex".

## Testing Decisions

- Tests target external behavior at the highest existing seams; implementation details (which match arm ran, internal call order) are not asserted.
- **Provider types seam (existing Rust unit-test module, primary):** serde round-trips for the new variant; descriptor-id-vs-serde drift guard iterating `AgentProvider::ALL`; `PerProvider` round-trip including legacy JSON with `null` values and unknown provider keys; fallback-model resolution as a data lookup. Prior art: the existing serialization tests for ClaudeCode/GeminiCli in the same module.
- **Candidate-path seam (the one new seam):** discovery splits into a pure `candidate_paths(descriptor, custom_path)` function (unit-testable precedence: env override > custom > known > home-relative) and a thin "first path that exists" filesystem check. Availability checking and spawning remain thin untested wrappers over it.
- **Provider selector seam (existing frontend component test):** extend with a mocked availability response including Codex, asserting the rendered option, disabled state, and error tooltip. Prior art: the existing ProviderSelector test.
- **Manual acceptance (not automated):** live streaming session through a real codex-acp install with ChatGPT login; requires the adapter and login state, so it stays a release-checklist item.

## Out of Scope

- Migrating Claude Code off the bundled sidecar to a user-installed adapter ([issue #11](https://github.com/davidchris/thoughttree/issues/11); see ADR-0001 consequences).
- ACP session config-options support (dynamic Codex model list, in-session model switching).
- Any auth handling inside ThoughtTree: no login flow, no auth pre-flight, no key storage.
- Per-node provider/model badges or other UI beyond the existing selector/settings surfaces.
- Summary generation via Codex (summaries remain Claude-only).

## Further Notes

Implementation-time verifications flagged during design (none block the design):

- Confirm the exact static model list against the current codex release before shipping (era of `gpt-5.1-codex` / `gpt-5.1-codex-mini`).
- `codex-acp` may not define a clap `--version` flag; the validation pattern check likely still passes on the usage/error output (contains "codex"), but the displayed version string may need a `--help` fallback.
- Confirm the known-path set for npm-global installs of `codex-acp` on macOS (homebrew node, `~/.npm-global`, `~/.bun`, nvm).

Key upstream references: [zed-industries/codex-acp](https://github.com/zed-industries/codex-acp) (adapter, npm `@zed-industries/codex-acp`), codex `-c key=value` config overrides (`CliConfigOverrides`).
