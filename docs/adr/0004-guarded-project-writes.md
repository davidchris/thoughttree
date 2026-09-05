# Guarded (compare-and-swap) project writes; server vault access via read-only mount + WebDAV

Three writers can mutate the same Nextcloud-synced `.thoughttree` file: desktop Tauri (local synced copy), the shape C server (NAS copy), and torhaus's patch flow (which already ships base-hash conflict checks, single-use patches, TTL). Today's desktop save is a bare full-file `std::fs::write` — no atomicity, no staleness check; last writer wins silently.

We decided every persist of a Project file is a **Guarded write**: write to a temp file, atomically rename into place, conditioned on the content hash captured at last read still matching the file. A stale write is rejected and the writer reloads and reapplies. The implementation lives once, in `thoughttree-core`'s vault FS, so desktop and server inherit it identically, and the convention is compatible with torhaus's existing base-hash discipline. No owner lease or lock file in v1 — single-user reality means reject-stale-plus-reload covers the realistic conflict (same graph open on desktop and iPad); lease machinery (heartbeats, takeover UX) is deferred until real conflicts bite.

For shape C, the vault is mounted **read-only**; writes go through Nextcloud WebDAV (the torhaus ADR 0001 pattern, proven on Wilde-NAS). Nextcloud stays the sync authority — no `occ files:scan`, changes propagate to all sync clients immediately — and WebDAV's `If-Match` ETag provides the compare-and-swap transport. Consequence: core's vault FS needs a storage seam with two backends, local FS (desktop) and RO-mount-reads-plus-WebDAV-writes (server).

## Considered options

- **Status-quo last-writer-wins** — zero work, silent data loss on same-copy races; rejected.
- **CAS plus advisory owner lease** (sidecar file, heartbeat, takeover) — prevents cross-device races earlier, but adds lease lifecycle complexity for a conflict pattern that may never materialize for a single user; deferred, not rejected.
- **Server as single writer** (desktop becomes a shape C client for vault graphs) — cleanest consistency, but kills offline desktop use and forces a much larger architecture shift; rejected.
- **Read-write NAS mount** — one storage backend instead of two, but direct writes into Nextcloud-managed data desync its state unless rescanned; rejected in favor of the proven torhaus pattern.

## Consequences

- Nextcloud remains the cross-device reconciliation boundary. CAS makes each copy internally safe; true simultaneous edits of desktop-local and NAS copies still surface as Nextcloud conflict files — visible, not silent.
- Desktop save must migrate from bare `fs::write` to the core Guarded write when `thoughttree-core` is extracted.
- Rejected saves become a user-visible state the frontend must handle (reload-and-reapply flow).
- Torhaus needs no changes; its patch flow already honors base-hash semantics.

## Amendment (2026-09): lock around check-and-replace

The original local-FS implementation checked the revision, then wrote a temp file and renamed it into place with nothing holding the two steps together. A second guarded writer could pass its own check in that window and be silently overwritten by the pending rename, which is exactly the loss CAS exists to prevent.

The local backend now holds an exclusive advisory lock (`std::fs::File::lock`) on a persistent sibling `.<name>.lock` file for the duration of check plus rename. A concurrent guarded writer blocks, then re-checks against the new content hash and receives the normal stale rejection. The lock file is never deleted: unlinking it would let a third writer create a fresh inode and bypass a waiter still blocked on the old one. It is not an owner lease; it is held only for the milliseconds a single write takes, and non-cooperating writers (editors, sync clients) are still reconciled by Nextcloud as before.
