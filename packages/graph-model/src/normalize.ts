import type {
  AssistantGraphNode,
  GraphAgentProvider,
  GraphNode,
  ImageAttachment,
  ProvenanceCompleteness,
  ToolActivityKind,
  ToolActivityStatus,
  TurnActivity,
  TurnProvenance,
  TurnReference,
  TurnReferenceRelation,
  UserGraphNode,
} from './types';

/**
 * Rebuilds GraphNodes from an explicit field allowlist so that raw tool
 * input/output, unknown payloads, absolute host paths, or provenance on user
 * nodes never cross the persistence boundary in either direction.
 */

// Allowlists are derived from exhaustive records so adding a union member
// without updating the allowlist fails to compile instead of silently
// dropping data on the next save.
function keysOf<T extends string>(record: Record<T, true>): ReadonlySet<T> {
  return new Set(Object.keys(record) as T[]);
}

const RELATIONS = keysOf<TurnReferenceRelation>({
  consulted: true,
  cited: true,
  read: true,
  created: true,
  updated: true,
  deleted: true,
  moved: true,
  searched: true,
  fetched: true,
});

const COMPLETENESS = keysOf<ProvenanceCompleteness>({ complete: true, partial: true, unknown: true });

const TOOL_KINDS = keysOf<ToolActivityKind>({
  read: true,
  edit: true,
  delete: true,
  move: true,
  search: true,
  execute: true,
  fetch: true,
  delegate: true,
  other: true,
});

const TOOL_STATUSES = keysOf<ToolActivityStatus>({
  pending: true,
  completed: true,
  failed: true,
  incomplete: true,
});

const PROVIDERS = keysOf<GraphAgentProvider>({ 'claude-code': true, 'gemini-cli': true, codex: true });

/** Tool titles are display summaries, never raw commands; cap them like the UI does. */
export const TOOL_TITLE_MAX_LENGTH = 200;

/** Generic per-kind summaries used when a tool title looks like a raw command or carries a host path. */
const TOOL_TITLE_SUMMARIES: Record<ToolActivityKind, string> = {
  read: 'Read a file',
  edit: 'Edited a file',
  delete: 'Deleted a file',
  move: 'Moved a file',
  search: 'Ran a search',
  execute: 'Ran a command',
  fetch: 'Fetched a resource',
  delegate: 'Delegated a task',
  other: 'Used a tool',
};

// An absolute host path token: `/Users/...`, `~/...`, `C:\...`, or a UNC path at
// the start of the text or after a delimiter. Relative paths (`src/App.tsx`) pass.
const HOST_PATH = /(?:^|[\s"'`(=:,<\[])(?:\/[^\s/]|~[\\/]|[A-Za-z]:[\\/]|\\\\)/;

// Shell operators and substitutions that mark a title as a raw command rather than a summary.
const SHELL_OPERATOR = /[`$|;<>]|&&|[\u0000-\u001f\u007f]/;

/** True when text carries an absolute host path or a `file:` URL. */
export function containsHostPath(text: string): boolean {
  return isFileUrlOrBarePath(text) || HOST_PATH.test(text);
}

/** True for `file:` URLs and bare absolute paths, which are never acceptable URL reference text. */
function isFileUrlOrBarePath(text: string): boolean {
  return /^\s*(?:file:|\/|~[\\/]|[A-Za-z]:[\\/]|\\\\)/i.test(text);
}

/**
 * Reduces an untrusted tool title to a safe display summary: raw commands and
 * host paths are replaced by a generic per-kind summary, everything else is
 * whitespace-collapsed and capped. `redacted` reports the replacement so the
 * caller can record the loss.
 */
export function safeToolTitle(rawTitle: string, kind: ToolActivityKind): { title: string; redacted: boolean } {
  const collapsed = rawTitle.replace(/\s+/g, ' ').trim();
  if (collapsed.length === 0 || SHELL_OPERATOR.test(collapsed) || containsHostPath(collapsed)) {
    return { title: TOOL_TITLE_SUMMARIES[kind], redacted: true };
  }
  return { title: collapsed.slice(0, TOOL_TITLE_MAX_LENGTH), redacted: false };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function str(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function num(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function bool(value: unknown): boolean | undefined {
  return typeof value === 'boolean' ? value : undefined;
}

function withOptional<T extends object>(base: T, extras: Record<string, unknown>): T {
  const result: Record<string, unknown> = { ...(base as Record<string, unknown>) };
  for (const [key, value] of Object.entries(extras)) {
    if (value !== undefined) result[key] = value;
  }
  return result as T;
}

/** Counts evidence discarded or reduced during normalization so completeness can be downgraded. */
class Loss {
  count = 0;
  note(): void {
    this.count += 1;
  }
}

function relations(value: unknown, loss: Loss): TurnReferenceRelation[] {
  if (!Array.isArray(value)) return [];
  const kept = value.filter((entry): entry is TurnReferenceRelation =>
    typeof entry === 'string' && RELATIONS.has(entry as TurnReferenceRelation)
  );
  if (kept.length !== value.length) loss.note();
  return kept;
}

/** Strips directory components from a file display name; host paths never survive as names. */
function safeDisplayName(value: string | undefined, loss: Loss): string | undefined {
  if (value === undefined) return undefined;
  const name = value.split(/[\\/]/).pop()?.trim() ?? '';
  if (name !== value) loss.note();
  return name.length > 0 && name !== '..' && name !== '.' ? name : undefined;
}

/** Drops an optional text field that would carry a host path into the Project file. */
function safeText(value: string | undefined, loss: Loss): string | undefined {
  if (value === undefined) return undefined;
  if (containsHostPath(value)) {
    loss.note();
    return undefined;
  }
  return value;
}

/**
 * Vault paths must stay inside the Vault: relative, no drive letters or URL
 * schemes, no parent-directory segments.
 */
export function isVaultRelativePath(path: string): boolean {
  if (path.length === 0) return false;
  if (path.startsWith('/') || path.startsWith('\\')) return false;
  // Rejects both URL schemes (file:, vault:) and Windows drive letters (C:).
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(path)) return false;
  return !path.split(/[\\/]/).some((segment) => segment === '..');
}

function reference(value: unknown, loss: Loss): TurnReference | undefined {
  if (!isRecord(value)) return undefined;
  const base = { relations: relations(value.relations, loss) };
  const timestamp = num(value.timestamp);

  if (value.type === 'url') {
    const url = str(value.url);
    // URL text is preserved exactly, but file: URLs and bare host paths are not URLs we keep.
    if (url === undefined || isFileUrlOrBarePath(url)) return undefined;
    return withOptional({ type: 'url' as const, url, ...base }, {
      title: safeText(str(value.title), loss),
      domain: safeText(str(value.domain), loss),
      index: num(value.index),
      percentage: num(value.percentage),
      is_search_result: bool(value.is_search_result),
      timestamp,
    });
  }

  if (value.type === 'file') {
    const path = str(value.path);
    const displayName = safeDisplayName(str(value.displayName) ?? path, loss);
    if (displayName === undefined) return undefined;
    if (value.scope === 'vault' && path !== undefined && isVaultRelativePath(path)) {
      return withOptional({ type: 'file' as const, scope: 'vault' as const, path, displayName, ...base }, { timestamp });
    }
    // External files, and vault references whose path escapes the Vault, keep only a display name.
    if (value.scope === 'vault') loss.note();
    return withOptional({ type: 'file' as const, scope: 'external' as const, displayName, ...base }, { timestamp });
  }

  return undefined;
}

function activityEntry(value: unknown, loss: Loss): TurnActivity | undefined {
  if (!isRecord(value)) return undefined;
  const timestamp = num(value.timestamp);

  if (value.type === 'commentary') {
    const content = str(value.content);
    if (content === undefined) return undefined;
    return withOptional({ type: 'commentary' as const, content }, { timestamp });
  }

  if (value.type === 'tool') {
    const rawTitle = str(value.title);
    if (rawTitle === undefined) return undefined;
    const rawKind = str(value.kind);
    const status = str(value.status);
    const kind: ToolActivityKind =
      rawKind !== undefined && TOOL_KINDS.has(rawKind as ToolActivityKind) ? (rawKind as ToolActivityKind) : 'other';
    const { title, redacted } = safeToolTitle(rawTitle, kind);
    const titleTruncated = !redacted && (value.titleTruncated === true || rawTitle.trim().length > TOOL_TITLE_MAX_LENGTH);
    if (redacted || titleTruncated) loss.note();
    return withOptional(
      {
        type: 'tool' as const,
        kind,
        title,
        status:
          status !== undefined && TOOL_STATUSES.has(status as ToolActivityStatus)
            ? (status as ToolActivityStatus)
            : 'incomplete',
      },
      {
        titleTruncated: titleTruncated ? true : undefined,
        titleRedacted: redacted ? true : undefined,
        completedAt: num(value.completedAt),
        timestamp,
      }
    );
  }

  if (value.type === 'unknown') {
    const providerType = str(value.providerType);
    const label = str(value.label);
    if (providerType === undefined || label === undefined) return undefined;
    return withOptional({ type: 'unknown' as const, providerType, label }, { timestamp });
  }

  return undefined;
}

function normalizeList<T>(value: unknown, parse: (entry: unknown, loss: Loss) => T | undefined, loss: Loss): T[] {
  if (!Array.isArray(value)) return [];
  const kept: T[] = [];
  for (const entry of value) {
    const parsed = parse(entry, loss);
    if (parsed === undefined) loss.note();
    else kept.push(parsed);
  }
  return kept;
}

/**
 * A `complete` claim only survives when nothing was discarded or reduced on
 * the way through; any known loss (dropped entries, redacted titles, demoted
 * Vault paths, retained Unknown activity) downgrades it to `partial`.
 * `partial` and `unknown` are never upgraded.
 */
export function normalizeProvenance(value: unknown): TurnProvenance | undefined {
  if (!isRecord(value)) return undefined;
  const loss = new Loss();
  const claimed = str(value.completeness);
  const references = normalizeList(value.references, reference, loss);
  const activity = normalizeList(value.activity, activityEntry, loss);
  if (activity.some((entry) => entry.type === 'unknown')) loss.note();

  let completeness: ProvenanceCompleteness =
    claimed !== undefined && COMPLETENESS.has(claimed as ProvenanceCompleteness)
      ? (claimed as ProvenanceCompleteness)
      : 'unknown';
  if (completeness === 'complete' && loss.count > 0) completeness = 'partial';

  return { completeness, references, activity };
}

function images(value: unknown): ImageAttachment[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const result: ImageAttachment[] = [];
  for (const entry of value) {
    if (!isRecord(entry)) continue;
    const data = str(entry.data);
    const mimeType = str(entry.mimeType);
    if (data === undefined || mimeType === undefined) continue;
    result.push(withOptional({ data, mimeType }, { name: str(entry.name) }));
  }
  return result;
}

export function normalizeGraphNode(value: unknown): GraphNode | undefined {
  if (!isRecord(value)) return undefined;
  const id = str(value.id);
  const content = str(value.content);
  const timestamp = num(value.timestamp);
  if (id === undefined || content === undefined || timestamp === undefined) return undefined;

  const shared = {
    contentUpdatedAt: num(value.contentUpdatedAt),
    summary: str(value.summary),
    summaryTimestamp: num(value.summaryTimestamp),
  };

  if (value.role === 'user') {
    const node: UserGraphNode = { id, role: 'user', content, timestamp };
    return withOptional(node, { ...shared, images: images(value.images) });
  }

  if (value.role === 'assistant') {
    const provider = str(value.provider);
    const node: AssistantGraphNode = { id, role: 'assistant', content, timestamp };
    return withOptional(node, {
      ...shared,
      provider: provider !== undefined && PROVIDERS.has(provider as GraphAgentProvider) ? provider : undefined,
      model: str(value.model),
      incomplete: value.incomplete === true ? true : undefined,
      provenance: normalizeProvenance(value.provenance),
    });
  }

  return undefined;
}
