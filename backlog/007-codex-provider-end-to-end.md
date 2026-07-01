# Codex provider end-to-end (adapter default model)

Triage: ready-for-agent

## Parent

[backlog/005-codex-provider-prd.md](./005-codex-provider-prd.md) (PRD for [issue #10](https://github.com/davidchris/thoughttree/issues/10))

## What to build

Add Codex as a third Provider, streaming through the user-installed `codex-acp` ACP adapter (`npm i -g @zed-industries/codex-acp`) per ADR-0001. Thanks to the descriptor prefactor this is one enum variant, one descriptor entry, and one plain-adapter spawn arm — the dropdown, availability check, install hint, settings path picker, and validation all derive from data.

- Codex appears in the provider dropdown (serde id `codex`, display name "Codex"). Unavailable state shows the two-step install hint: adapter install plus login via the vendor CLI (`npm i -g @openai/codex && codex login`).
- Availability = adapter executable found, nothing more. No auth pre-flight; a logged-out user gets the adapter's auth error at session start, surfaced in the chat panel like any session error.
- Spawn: launch the discovered adapter plainly over stdio (no sidecar, no ACP flag); sessions use the user's own codex config default model. Model selection is a separate slice.
- Custom executable path configurable in Settings with validation. Note: the adapter may not define a `--version` flag — validation pattern matching on combined output should still pass (usage text contains "codex"); verify live and fall back to a `--help` probe for the displayed version string if needed.
- Model discovery for Codex returns empty (the adapter exposes models via ACP config options, which ThoughtTree deliberately does not consume — see PRD); with an empty `fallback_models` list in this slice, the model selector simply has nothing to offer yet.

## Acceptance criteria

- [x] Codex selectable in the provider dropdown; disabled with actionable install hint when the adapter is missing
- [ ] With adapter installed and `codex login` done: user sends a prompt and the response streams into the GraphNode
- [ ] Branching/merging Codex conversations in the Graph works like other providers
- [ ] Logged-out state produces a clear session-start error mentioning login, not a crash or silent hang
- [x] Custom path set in Settings is validated, persisted, and used for discovery (validation verified against real adapter output: `--version` is unrecognized but usage text validates; the `Usage:` line is shown instead of the error line)
- [x] Serde round-trip and descriptor drift tests cover the new variant; provider selector test covers the Codex option incl. unavailable state
- [ ] Manual acceptance: live streaming session on a machine with real adapter + ChatGPT login

## Blocked by

- [backlog/006-provider-descriptor-prefactor.md](./006-provider-descriptor-prefactor.md)
