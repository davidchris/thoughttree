# PRD: ThoughtTree Mobile Push (v1)

**Status:** Draft, grilled 2026-07-07
**Strategy record:** vault note `2026-07-06-175349_thoughttree-mobile-strategy-exploration.md` — full option analysis, architecture rationale, torhaus boundary, backburner. This PRD does not repeat it.
**Vocabulary:** `CONTEXT.md` (Deployment shape, Project file, Vault, Guarded write, Turn, Attachment, Parked permission).

## Goal

ThoughtTree usable interactively on iPad: live streaming ACP sessions on the existing canvas, served from the Synology NAS over Tailscale. iOS forbids subprocesses, so the agent runtime moves server-side (Deployment shape C) behind a shared `thoughttree-core` crate that also keeps serving the desktop app unchanged (shape A).

## v1 scope

**In:**

- `GraphModel` extracted as a standalone TS package (pure, client-side, unchanged ownership of conversation-path building).
- Cargo workspace split: `crates/thoughttree-core` (ACP sessions, vault FS with Guarded writes, permission broker), `crates/thoughttree-tauri` (desktop shell), `crates/thoughttree-server` (axum HTTP/WebSocket binary).
- Desktop save migrates to Guarded writes (ADR 0004) — the boy-scout dividend of core extraction.
- Shape C server on Wilde-NAS: Vault via read-only mount + Nextcloud WebDAV writes (ADR 0004), existing React frontend served as PWA, reachable tailnet-only.
- Server-owned Turns: a prompt runs to completion regardless of client connection; stream buffered and applied to the graph server-side; reconnect = snapshot + resubscribe. Multiple Attachments may watch one graph; one active Turn per graph, concurrent prompts rejected.
- Parked permissions: requests with no attached client pend indefinitely (no timeout, no auto-deny) and are surfaced first on reattach.

**Out (deferred, not rejected):**

- Mobile-first thread view for iPhone (torhaus covers the iPhone asynchronous niche today).
- Shape B laptop-as-server toggle (no evidenced laptop-only users yet; same server code when it comes).
- Web push for parked permissions (iOS PWA push is viable ≥16.4; VAPID/subscription infra is v2).
- Owner lease / cross-device write arbitration beyond Guarded writes (revisit if Nextcloud conflict files actually appear).
- Permission auto-allow policy (tracked separately, `backlog/002-permission-config.md`).
- GPUI shell (backburner; core extraction keeps it open by design).

## Decisions

| Question | Decision | Where recorded |
|---|---|---|
| Concurrent writes | Guarded (CAS) writes in core; no lease | ADR 0004 |
| Server vault access | RO mount + WebDAV writes, If-Match ETag as CAS transport | ADR 0004 |
| Session lifecycle | Server-owned detachable Turns, view fan-out, one Turn per graph | this PRD |
| Permissions unattended | Park, no timeout | this PRD |
| License | MIT app/core/GraphModel, AGPL-3.0 server from first commit | ADR 0005 |
| NAS cohabitation | Standalone compose stack; own tailnet node via `tailscale/tailscale` sidecar (tsnet is Go-only); own `CLAUDE_CONFIG_DIR` volume; same `CLAUDE_CODE_OAUTH_TOKEN` value duplicated in env | this PRD |
| Torhaus boundary | Parallel implementation; torhaus M3 untouched; overlap limited to `.thoughttree` parsing + continue-branch primitive | strategy note |
| Validation | PRD now; mobile-facing milestones gated on stopgap evidence | this PRD, M-gate below |

## Milestones

Extraction milestones are unconditional — they pay off regardless of mobile (shape B, GPUI option, testability).

1. **M1 — GraphModel package.** `packages/graph-model`, consumed by the existing frontend, zero behavior change.
2. **M2 — Core extraction.** Cargo workspace; `thoughttree-core` owns ACP sessions, vault FS, permission broker; Tauri shell consumes it in-process. Desktop ships from the workspace with no user-visible change.
3. **M3 — Guarded writes.** Core vault FS implements ADR 0004 (local-FS backend); desktop save migrated; stale-save reject + reload flow in the frontend.
4. **GATE — validation evidence.** Remote-desktop-over-Tailscale stopgap (plan step 7) has run and shows real mobile ThoughtTree usage. If usage isn't real: stop here — M1–M3 already paid for themselves.
5. **M4 — Server crate.** `thoughttree-server` (AGPL): axum HTTP/WS surface over core, server-owned Turns with buffer/replay, Parked permissions, WebDAV storage backend, vault listing endpoint.
6. **M5 — iPad PWA.** Existing React frontend served by the server; attach/reattach flow; PWA manifest; tested on iPad over Tailscale.
7. **M6 — NAS deploy.** Standalone compose stack on Wilde-NAS per the torhaus runbook pattern (`~/dev/torhaus/deploy/README.md` as template): tailscaled sidecar, RO vault mount, env-based token, tagged rollback.

## Assumptions

- **Auth = tailnet membership.** v1 has no app-level auth; reachability via Tailscale is the perimeter, same posture as torhaus. Revisit only if the server ever leaves the tailnet.
- Nextcloud remains sync authority for the Vault; ThoughtTree never reconciles cross-device conflicts itself (Nextcloud conflict files are the visible residual).
- Claude auth on the NAS uses the proven torhaus pattern (`claude setup-token` → env var, persistent config dir).

## Risks

- **Stream buffering/replay is new surface** — the desktop path never needed it; most of M4's unknown budget lives there.
- **WebDAV CAS semantics** — If-Match/ETag behavior under Nextcloud needs a spike early in M4; fallback is GET-hash-compare-PUT with a narrower race window.
- **PWA canvas ergonomics on iPad** (touch targets, pinch-zoom vs ReactFlow gestures) — unknown until M5; the stopgap evidence from the GATE partially de-risks it.
