import { useEffect, useState } from 'react';
import { getBackendTransport } from '../lib/transport';
import type { FilePreviewResponse } from '../lib/transport';
import type { FileNodeData } from '../types';

/** Backend refusals the card renders as a state instead of an error. */
export type PreviewRefusal = 'too-large' | 'missing' | 'invalid';

export type PreviewState =
  | { kind: 'loading' }
  | { kind: 'ready'; response: FilePreviewResponse }
  | { kind: 'refused'; reason: PreviewRefusal }
  | { kind: 'failed' };

// Keyed by the version the node has seen, so acknowledging a change (new
// seenMtime/seenSize) naturally fetches a fresh preview. Failures are not
// cached: a file that reappears must not stay "missing" until its key changes.
const previewCache = new Map<string, Promise<FilePreviewResponse>>();
const PREVIEW_CACHE_MAX = 64;

export function previewKey(node: Pick<FileNodeData, 'path' | 'seenMtime' | 'seenSize'>): string {
  return `${node.path}:${node.seenMtime}:${node.seenSize}`;
}

export function resetFilePreviewCache(): void {
  previewCache.clear();
}

function fetchPreview(node: FileNodeData): Promise<FilePreviewResponse> {
  const key = previewKey(node);
  const cached = previewCache.get(key);
  if (cached) return cached;
  if (previewCache.size >= PREVIEW_CACHE_MAX) previewCache.clear();
  const pending = getBackendTransport().readVaultFilePreview(node.path);
  previewCache.set(key, pending);
  pending.catch(() => previewCache.delete(key));
  return pending;
}

const REFUSAL_PREFIXES: Array<[string, PreviewRefusal]> = [
  ['too_large:', 'too-large'],
  ['missing:', 'missing'],
  ['invalid:', 'invalid'],
];

export function previewRefusal(error: unknown): PreviewRefusal | null {
  const message = error instanceof Error ? error.message : String(error);
  return REFUSAL_PREFIXES.find(([prefix]) => message.startsWith(prefix))?.[1] ?? null;
}

/** Bounded preview of a file node's Vault file, shared by the canvas card and the side panel. */
export function useFilePreview(node: FileNodeData | undefined): PreviewState {
  const key = node ? previewKey(node) : null;
  const [state, setState] = useState<PreviewState>({ kind: 'loading' });

  useEffect(() => {
    if (!node) return;
    let cancelled = false;
    setState({ kind: 'loading' });
    fetchPreview(node).then(
      (response) => {
        if (!cancelled) setState({ kind: 'ready', response });
      },
      (error: unknown) => {
        if (cancelled) return;
        const reason = previewRefusal(error);
        setState(reason ? { kind: 'refused', reason } : { kind: 'failed' });
      }
    );
    return () => {
      cancelled = true;
    };
    // `key` captures every field the fetch depends on (path, seenMtime, seenSize).
  }, [key]);

  return node ? state : { kind: 'loading' };
}
