# New providers use user-installed ACP adapters, not bundled sidecars

ThoughtTree's first provider (Claude Code) ships its ACP adapter (`claude-code-acp`) as a bundled sidecar binary. When adding Codex (2026-07), we decided new providers instead require a user-installed adapter executable (e.g. `npm i -g @zed-industries/codex-acp`), discovered via known install paths like the Gemini CLI — bundling was rejected because it inflates the app bundle, ties our release cadence to upstream adapter releases, and complicates the per-platform build pipeline for marginal UX gain (the availability check + install hint already handles the missing-binary case).

## Consequences

- The bundled `claude-code-acp` sidecar is legacy, not the template. [Issue #11](https://github.com/davidchris/thoughttree/issues/11) tracks migrating Claude Code to the user-installed pattern too (breaking change for existing users, needs its own release + comms).
- Availability = adapter executable found. Auth (`codex login`, ChatGPT subscription) is the vendor CLI's job and surfaces as an error at session start, never as a pre-flight check in ThoughtTree.
