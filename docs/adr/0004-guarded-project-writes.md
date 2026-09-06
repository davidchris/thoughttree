# Local guarded Project writes and independent recovery

The September 2026 decision replaces the earlier strict-CAS requirement and no-lock rule for local writes. ThoughtTree keeps local and offline saves. The strict CAS ticket `thoughttree-m0p.1` remains open. This change does not require a remote backend or platform-specific atomic exchange.

## Local writer protocol

Every ThoughtTree Project writer uses the core vault functions. All writers share one machine-local advisory lock, held exclusively from revision validation through atomic replacement. A shared/read lock is insufficient because it permits simultaneous writers. One lock for all Projects avoids distinct locks for path aliases. The lock inode remains in local application data and is never unlinked during normal operation. It is outside the synced Vault, has no owner lease, and is released when the process closes its handle or exits.

The protocol directory is `thoughttree/project-state-v1` under the operating system's local application-data directory. `THOUGHTTREE_LOCAL_STATE_DIR` overrides it for isolated tests or deployments. All cooperating processes must use the same directory and OS account. An older application version that does not implement this protocol does not participate in its protection.

Before any Project write attempt, core preserves the proposed content in a recovery snapshot. It then acquires the lock and compares the current content hash with the loaded revision. It writes and syncs a temporary sibling, rechecks the hash, and atomically renames the temporary file over the Project. A detected mismatch rejects the write. A null revision creates a new file only: it does not authorize overwriting an existing Project. The separate-copy action chooses a unique sibling name and uses this same protocol.

Desktop commands and the async local vault perform writes on a blocking pool. Snapshot or lock errors abort the write. The frontend serializes its save requests and keeps later edits dirty while an earlier request completes.

## Conflict handling

A detected conflict preserves the unsaved graph and stops automatic Project-save retries. The dialog offers Compare Versions, Reload, and Save a Separate Copy. Comparison is read-only and never adopts the disk revision for a subsequent save. Reload preserves a completed recovery snapshot before replacing the in-memory graph. If preservation fails or edits change during preservation, reload aborts. A separate copy becomes the active Project without overwriting the original.

## Recovery scope

Recovery snapshots contain the complete serialized graph and Project preferences. They are independent of the synced Project and use content-addressed records under `project-state-v1/recovery`. Snapshot files are synced before atomic publication. The snapshot content hash is checked on read. Identical source/content pairs share a snapshot. This version does not automatically prune completed snapshots.

The frontend checkpoints dirty drafts at most once per second during edits, including streams and conflicts. Each write attempt also checkpoints its exact proposed content. Reload waits for an explicit checkpoint. The Recovery snapshots browser is available from both the toolbar and opening screen. A recovered snapshot opens as an unsaved Project, so recovery never automatically overwrites its original file.

Only completed snapshots are recoverable. A crash can lose edits since the most recent completed checkpoint. Snapshot failures remain visible, and destructive conflict reload is blocked when its checkpoint fails. Local recovery is not an off-machine backup and does not protect against loss of the application-data directory or storage failure.

## External writers and synchronization

Editors and sync clients can bypass the advisory lock. They can change a Project between the final hash check and replacement. This protocol is not strict CAS and cannot guarantee recovery of external versions ThoughtTree never observed. A completed ThoughtTree snapshot can recover ThoughtTree edits after a missed conflict or crash. Nextcloud remains responsible for cross-device reconciliation, but this decision makes no promise that every external conflict becomes a sync conflict file.

The planned WebDAV backend can use server-enforced conditional writes in a separate change. It is not a dependency of this local-write fix.
