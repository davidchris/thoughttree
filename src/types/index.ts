// ============================================================================
// Agent Provider Types
// ============================================================================

export type AgentProvider = 'claude-code' | 'gemini-cli';

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
}

export const PROVIDER_DESCRIPTORS: readonly ProviderDescriptor[] = [
  { id: 'claude-code', displayName: 'Claude Code', shortName: 'Claude' },
  { id: 'gemini-cli', displayName: 'Gemini CLI', shortName: 'Gemini' },
];

export const ALL_PROVIDERS: readonly AgentProvider[] = PROVIDER_DESCRIPTORS.map((d) => d.id);

export const PROVIDER_DISPLAY_NAMES: Record<AgentProvider, string> = Object.fromEntries(
  PROVIDER_DESCRIPTORS.map((d) => [d.id, d.displayName])
) as Record<AgentProvider, string>;

export const PROVIDER_SHORT_NAMES: Record<AgentProvider, string> = Object.fromEntries(
  PROVIDER_DESCRIPTORS.map((d) => [d.id, d.shortName])
) as Record<AgentProvider, string>;

export const DEFAULT_PROVIDER: AgentProvider = 'claude-code';

// ============================================================================
// Model Types
// ============================================================================

export interface ModelInfo {
  model_id: string;
  display_name: string;
}

export type ModelPreferences = Partial<Record<AgentProvider, string | null>>;

export type ProviderPaths = Partial<Record<AgentProvider, string | null>>;

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
  provider?: AgentProvider;   // Which provider generated this response
  model?: string;             // Which model was used for this response
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
