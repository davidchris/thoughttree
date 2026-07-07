# License split: MIT app/core/GraphModel, AGPL-3.0 server crate

ThoughtTree is MIT and published. The mobile push adds a server component (`thoughttree-server`, deployment shape C), and the business posture — app free OSS forever, charge only for hosted convenience, if ever — makes the server the one place where license choice has commercial consequence. Relicensing later, after outside contributions arrive, means CLA bureaucracy or per-contributor consent; deciding at the crate's first commit is the cheap moment, and David is currently sole author.

We decided: the desktop app, `thoughttree-core`, and the GraphModel TS package stay **MIT**; `thoughttree-server` (crate and binary) is **AGPL-3.0 from its first commit**. MIT code can flow into an AGPL work, so the server linking core is clean. Anyone can self-host the server; nobody can build a closed-source hosted ThoughtTree on top of it.

## Considered options

- **All MIT** — maximum adoption and contributor ease; forfeits any license protection for a future hosted offering, leaving only an ops/brand moat; rejected.
- **All AGPL going forward** (including core) — strongest protection, but AGPL core infects the MIT desktop app and deters embedders and contributors everywhere, not just on the server; rejected.
- **Defer until shape C ships** — every accepted server contribution before the decision narrows options; rejected.

## Consequences

- The repo carries two licenses; the workspace layout must make the boundary obvious (license file in the server crate, root README note).
- Contributions to `thoughttree-server` are AGPL-encumbered from day one — no relicensing debt accumulates.
- A future hosted offering keeps the Obsidian-shaped option open without any CLA apparatus today.
- Code moved from server into core is a license *downgrade* (AGPL→MIT) and only safe while all server authors consent — one more reason to keep shared logic in core from the start.
