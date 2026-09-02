# ADR 0007: TypeScript single import path

- Status: Accepted
- Date: 2026-09-02

## Context

Kagi imports previously parsed the provider export and constructed Graph JSON in
both the Rust Tauri backend and the TypeScript frontend. That duplication means
the backend must track every change to the Graph model, project-file v4
serialization, provenance types, deterministic ids, and layout. It also prevents
the same provider-neutral import path from being reused by the PWA.

## Decision

TypeScript is the single Kagi import path. The parser in
`packages/graph-model/src/kagi.ts` produces the provider-neutral conversation
DTO, and `conversationToGraph` in `packages/graph-model/src/import.ts` is the
only DTO-to-Graph transform. The frontend transport invokes the Tauri command,
parses the returned text, and transforms the DTO into an `ImportedGraph`.

Rust keeps only a thin file seam: it validates and opens the selected path,
enforces the 16 MiB cap on the opened handle, reads the file, rejects malformed
UTF-8, and returns the text. It never parses Kagi data or constructs Graph JSON.

If future Codex session payloads are too large for IPC, a Rust adapter may emit
the small provider-neutral conversation DTO instead. The frontend will still
own the DTO-to-Graph transform.

## Consequences

- Graph semantics and deterministic import ids have one implementation.
- The parser and transform can be reused by the PWA without a Tauri backend.
- Tauri IPC carries raw export text and therefore has a bounded, testable file
  boundary.
- Rust no longer validates provider-specific Kagi structure; parser errors are
  surfaced by the frontend through the existing import error path.
