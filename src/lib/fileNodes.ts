import type { FileNodeStatus } from '../store/useGraphStore';
import type { PreviewState } from '../hooks/useFilePreview';

/** Display helpers for file nodes (pure, no I/O). */

export type FileCardState = FileNodeStatus['state'];

// Mirrors thoughttree_core::vault::files::is_raster_image exactly. Only these
// are read as bytes and inlined as image blocks, so only these are size-limited;
// every other file (svg included) is a pointer the agent reads itself.
const RASTER_IMAGE_MIMES: ReadonlySet<string> = new Set(['image/png', 'image/jpeg', 'image/gif', 'image/webp']);

export function isRasterImage(mimeType: string): boolean {
  return RASTER_IMAGE_MIMES.has(mimeType);
}

/** The stat result wins for broken files; otherwise a preview refusal (e.g. image side too big) shows. */
export function resolveFileCardState(status: FileNodeStatus | undefined, preview: PreviewState): FileCardState {
  const stat = status?.state ?? 'ok';
  if (stat === 'missing' || stat === 'invalid' || stat === 'too-large') return stat;
  if (preview.kind === 'refused') return preview.reason;
  return stat;
}

/** Short labels for the states a user must act on. */
export const FILE_STATE_LABEL: Partial<Record<FileCardState, string>> = {
  changed: 'changed on disk',
  missing: 'file missing',
  invalid: 'invalid path',
  'too-large': 'too large for the agent',
};

/** Uppercased extension for the type badge, `FILE` when the name has no usable extension. */
export function fileTypeBadge(name: string): string {
  const match = /\.([A-Za-z0-9]{1,5})$/.exec(name);
  return match ? match[1].toUpperCase() : 'FILE';
}

/** Human-readable byte count: `812 B`, `2.0 KB`, `1.4 MB`. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  return `${(kb / 1024).toFixed(1)} MB`;
}
