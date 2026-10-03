# Offline ACP fixture

[`gpui-fixture-agent.py`](../../scripts/gpui-fixture-agent.py) is an offline ACP adapter for desktop tests and screenshots.
Every response identifies its synthetic origin.
The adapter does not call a model, use the network, or read Vault contents.
It exercises the same subprocess, permission, streaming, and provenance code as a real Provider.

The adapter supports model discovery, model selection, successful responses, failed responses, and parked permissions.
It also supports the `codex exec --ephemeral` command that ThoughtTree uses for summaries.
Its summary response is always `Deterministic Fixture Response`.

To configure an isolated desktop demonstration:

1. Copy the [shared Project fixture](fixtures/README.md) into a temporary Vault.
2. Create `fixture-notes.md` inside that Vault with synthetic text.
3. Create a temporary config directory with this `config.json`:

```json
{
  "notes_directory": "/absolute/path/to/temporary/vault",
  "default_provider": "codex",
  "provider_paths": {
    "codex": "/absolute/path/to/repo/scripts/gpui-fixture-agent.py"
  },
  "model_preferences": { "codex": "fixture-alternate" },
  "effort_preferences": { "codex": "xhigh" }
}
```

4. Set the three environment variables before you start the native app:

```sh
export THOUGHTTREE_CONFIG_DIR=/absolute/path/to/temporary/config
export THOUGHTTREE_LOCAL_STATE_DIR=/absolute/path/to/temporary/recovery
export CODEX_PATH=/absolute/path/to/repo/scripts/gpui-fixture-agent.py
cargo run -p thoughttree-gpui -- /absolute/path/to/temporary/vault/parity.thoughttree
```

`CODEX_PATH` routes automatic summaries to the fixture instead of an installed Codex CLI.
The config override isolates application preferences.
The local-state override isolates Recovery snapshots and writer locks.

| Prompt text | Fixture behavior |
| --- | --- |
| `parity` | Streams synthetic text, parks a WebFetch permission, then resumes after the selected response. |
| `fixture:nopermission` | Completes the response without a permission request. |
| `fixture:fail` | Returns an ACP error after synthetic activity, with partial provenance. |

The fixture reports a synthetic read of `fixture-notes.md` without opening it.
Core resolves that existing file into a Vault reference.
The final response includes a table, Rust code, math, and a Mermaid diagram.

Run the subprocess integration tests with:

```sh
cargo test -p thoughttree-desktop --test acp
```

The tests create temporary config and Vault directories.
They cover ordered stream events, completion ordering, permissions, concurrent independent Turns, error recovery, model discovery, executable validation, and ephemeral summaries.
Timeouts bound test failures. The tests do not use sleeps.

These tests prove the native service's ACP integration with the fixture.
They do not prove live Provider compatibility or complete frontend parity.
