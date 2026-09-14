import type { TurnProvenance } from '@thoughttree/graph-model';

// ============================================================================
// Agent Provider Types
// ============================================================================

export type AgentProvider = 'claude-code' | 'codex';
export type ReasoningEffort = 'low' | 'medium' | 'high' | 'xhigh';

export interface ProviderStatus {
  provider: AgentProvider;
  available: boolean;
  error_message: string | null;
}

/// Static per-provider data, mirroring the Rust ProviderDescriptor table
/// (src-tauri/src/backend/types.rs). Adding a Provider means adding one entry.
export interface ProviderDescriptor {
  id: AgentProvider;
  displayName: string;
  shortName: string;
  supportedEfforts: readonly ReasoningEffort[];
}

export const PROVIDER_DESCRIPTORS: readonly ProviderDescriptor[] = [
  {
    id: 'claude-code',
    displayName: 'Claude Code',
    shortName: 'Claude',
    supportedEfforts: ['low', 'medium', 'high', 'xhigh'],
  },
  {
    id: 'codex',
    displayName: 'Codex',
    shortName: 'Codex',
    supportedEfforts: ['low', 'medium', 'high', 'xhigh'],
  },
];

export const ALL_PROVIDERS: readonly AgentProvider[] = PROVIDER_DESCRIPTORS.map((d) => d.id);

const byProvider = <T>(pick: (d: ProviderDescriptor) => T): Record<AgentProvider, T> =>
  Object.fromEntries(PROVIDER_DESCRIPTORS.map((d) => [d.id, pick(d)])) as Record<
    AgentProvider,
    T
  >;

export const PROVIDER_DISPLAY_NAMES: Record<AgentProvider, string> = byProvider(
  (d) => d.displayName
);

export const PROVIDER_SHORT_NAMES: Record<AgentProvider, string> = byProvider((d) => d.shortName);

/** Historical messages keep attribution even after a provider is removed. */
export function providerShortName(provider: string | undefined): string {
  if (!provider) return 'Assistant';
  return PROVIDER_DESCRIPTORS.find((d) => d.id === provider)?.shortName ?? 'Retired provider';
}

export const PROVIDER_SUPPORTED_EFFORTS: Record<
  AgentProvider,
  readonly ReasoningEffort[]
> = byProvider((d) => d.supportedEfforts);

export const DEFAULT_PROVIDER: AgentProvider = 'codex';

// ============================================================================
// Model Types
// ============================================================================

export interface ModelInfo {
  model_id: string;
  display_name: string;
}

export type ModelPreferences = Partial<Record<AgentProvider, string>>;
export type EffortPreferences = Partial<Record<AgentProvider, ReasoningEffort>>;

export type ProviderPaths = Partial<Record<AgentProvider, string>>;

/** Wire/on-disk shape: the backend and legacy project files store explicit
 * nulls for unset entries. The UI treats null and missing identically, so
 * strip nulls at the boundary with {@link withoutNullEntries}. */
export type StoredProviderRecord<V extends string = string> = Partial<
  Record<AgentProvider, V | null>
>;

export function withoutNullEntries<V extends string>(
  record: StoredProviderRecord<V>
): Partial<Record<AgentProvider, V>> {
  return Object.fromEntries(
    Object.entries(record).filter(([provider, value]) =>
      ALL_PROVIDERS.includes(provider as AgentProvider) && value != null
    )
  ) as Partial<Record<AgentProvider, V>>;
}

// ============================================================================
// Node data types - discriminated union for user vs agent nodes
// ============================================================================

export interface ImageAttachment {
  data: string;      // Base64-encoded image data (no data: prefix)
  mimeType: string;  // e.g., "image/png", "image/jpeg"
  name?: string;     // Optional filename for display
}

export interface UserNodeData {
  id: string;
  role: 'user';
  content: string;
  timestamp: number;
  contentUpdatedAt?: number;  // When content was last edited/streamed
  summary?: string;           // Generated summary for collapsed view
  summaryTimestamp?: number;  // When summary was last generated
  images?: ImageAttachment[]; // Optional array of attached images
}

export interface AgentNodeData {
  id: string;
  role: 'assistant';
  content: string;
  timestamp: number;
  contentUpdatedAt?: number;  // When content was last edited/streamed
  summary?: string;           // Generated summary for collapsed view
  summaryTimestamp?: number;  // When summary was last generated
  provider?: import('@thoughttree/graph-model').GraphAgentProvider; // Historical attribution
  model?: string;             // Which model was used for this response
  provenance?: TurnProvenance;
  // Note: isStreaming is derived from store.streamingNodeId, not stored here
}

export type MessageNodeData = UserNodeData | AgentNodeData;

// Permission system
export interface PermissionOption {
  id: string;
  label: string;
}

export interface PermissionRequest {
  id: string;
  toolType: string;
  toolName: string;
  description: string;
  options: PermissionOption[];
}
