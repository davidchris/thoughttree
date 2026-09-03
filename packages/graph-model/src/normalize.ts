import type {
  AssistantGraphNode,
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

const RELATIONS: ReadonlySet<TurnReferenceRelation> = new Set<TurnReferenceRelation>([
  'consulted',
  'cited',
  'read',
  'created',
  'updated',
  'deleted',
  'moved',
  'searched',
  'fetched',
]);

const COMPLETENESS: ReadonlySet<ProvenanceCompleteness> = new Set<ProvenanceCompleteness>([
  'complete',
  'partial',
  'unknown',
]);

const TOOL_KINDS: ReadonlySet<ToolActivityKind> = new Set<ToolActivityKind>([
  'read',
  'edit',
  'delete',
  'move',
  'search',
  'execute',
  'fetch',
  'delegate',
  'other',
]);

const TOOL_STATUSES: ReadonlySet<ToolActivityStatus> = new Set<ToolActivityStatus>([
  'pending',
  'completed',
  'failed',
  'incomplete',
]);

/** Tool titles are display summaries, never raw commands; cap them like the UI does. */
export const TOOL_TITLE_MAX_LENGTH = 200;

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

function relations(value: unknown): TurnReferenceRelation[] {
  if (!Array.isArray(value)) return [];
  return value.filter((entry): entry is TurnReferenceRelation =>
    typeof entry === 'string' && RELATIONS.has(entry as TurnReferenceRelation)
  );
}

/**
 * Vault paths must stay inside the Vault: relative, no drive letters or URL
 * schemes, no parent-directory segments.
 */
export function isVaultRelativePath(path: string): boolean {
  if (path.length === 0) return false;
  if (path.startsWith('/') || path.startsWith('\\')) return false;
  if (/^[a-zA-Z]:[\\/]/.test(path)) return false;
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(path)) return false;
  return !path.split(/[\\/]/).some((segment) => segment === '..');
}

function reference(value: unknown): TurnReference | undefined {
  if (!isRecord(value)) return undefined;
  const base = { relations: relations(value.relations) };
  const timestamp = num(value.timestamp);

  if (value.type === 'url') {
    const url = str(value.url);
    if (url === undefined) return undefined;
    return withOptional({ type: 'url' as const, url, ...base }, {
      title: str(value.title),
      domain: str(value.domain),
      index: num(value.index),
      percentage: num(value.percentage),
      is_search_result: bool(value.is_search_result),
      timestamp,
    });
  }

  if (value.type === 'file') {
    const path = str(value.path);
    const displayName = str(value.displayName) ?? (path === undefined ? undefined : path.split(/[\\/]/).pop());
    if (displayName === undefined) return undefined;
    if (value.scope === 'vault' && path !== undefined && isVaultRelativePath(path)) {
      return withOptional({ type: 'file' as const, scope: 'vault' as const, path, displayName, ...base }, { timestamp });
    }
    // External files, and vault references whose path escapes the Vault, keep only a display name.
    return withOptional({ type: 'file' as const, scope: 'external' as const, displayName, ...base }, { timestamp });
  }

  return undefined;
}

function activity(value: unknown): TurnActivity | undefined {
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
    const kind = str(value.kind);
    const status = str(value.status);
    const title = rawTitle.slice(0, TOOL_TITLE_MAX_LENGTH);
    const titleTruncated = value.titleTruncated === true || rawTitle.length > TOOL_TITLE_MAX_LENGTH;
    return withOptional(
      {
        type: 'tool' as const,
        kind: kind !== undefined && TOOL_KINDS.has(kind as ToolActivityKind) ? (kind as ToolActivityKind) : 'other',
        title,
        status:
          status !== undefined && TOOL_STATUSES.has(status as ToolActivityStatus)
            ? (status as ToolActivityStatus)
            : 'incomplete',
      },
      { titleTruncated: titleTruncated ? true : undefined, completedAt: num(value.completedAt), timestamp }
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

export function normalizeProvenance(value: unknown): TurnProvenance | undefined {
  if (!isRecord(value)) return undefined;
  const completeness = str(value.completeness);
  return {
    completeness:
      completeness !== undefined && COMPLETENESS.has(completeness as ProvenanceCompleteness)
        ? (completeness as ProvenanceCompleteness)
        : 'unknown',
    references: Array.isArray(value.references)
      ? value.references.flatMap((entry) => reference(entry) ?? [])
      : [],
    activity: Array.isArray(value.activity)
      ? value.activity.flatMap((entry) => activity(entry) ?? [])
      : [],
  };
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

const PROVIDERS = new Set(['claude-code', 'gemini-cli', 'codex']);

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
      provider: provider !== undefined && PROVIDERS.has(provider) ? provider : undefined,
      model: str(value.model),
      incomplete: value.incomplete === true ? true : undefined,
      provenance: normalizeProvenance(value.provenance),
    });
  }

  return undefined;
}
