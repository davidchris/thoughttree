// ============================================================================
// Agent Provider Types
// ============================================================================

export type AgentProvider = 'claude-code' | 'gemini-cli';

export interface ProviderStatus {
  provider: AgentProvider;
  available: boolean;
  error_message: string | null;
}

export const PROVIDER_DISPLAY_NAMES: Record<AgentProvider, string> = {
  'claude-code': 'Claude Code',
  'gemini-cli': 'Gemini CLI',
};

export const PROVIDER_SHORT_NAMES: Record<AgentProvider, string> = {
  'claude-code': 'Claude',
  'gemini-cli': 'Gemini',
};

export const DEFAULT_PROVIDER: AgentProvider = 'claude-code';

// ============================================================================
// Model Types
// ============================================================================

export interface ModelInfo {
  model_id: string;
  display_name: string;
}

export interface ModelPreferences {
  'claude-code'?: string;
  'gemini-cli'?: string;
}

export interface ProviderPaths {
  'claude-code'?: string;
  'gemini-cli'?: string;
}

// ============================================================================
// Node data types - discriminated union for user vs agent nodes
// ============================================================================

export interface UserNodeData {
  id: string;
  role: 'user';
  content: string;
  timestamp: number;
  summary?: string;           // Generated summary for collapsed view
  summaryTimestamp?: number;  // When summary was last generated
}

export interface AgentNodeData {
  id: string;
  role: 'assistant';
  content: string;
  timestamp: number;
  summary?: string;           // Generated summary for collapsed view
  summaryTimestamp?: number;  // When summary was last generated
  provider?: AgentProvider;   // Which provider generated this response
  model?: string;             // Which model was used for this response
  // Note: isStreaming is derived from store.streamingNodeId, not stored here
}

export type MessageNodeData = UserNodeData | AgentNodeData;

// ReactFlow node data wrappers
export interface UserFlowNodeData extends Record<string, unknown> {
  nodeData: UserNodeData;
  isSelected: boolean;
}

export interface AgentFlowNodeData extends Record<string, unknown> {
  nodeData: AgentNodeData;
  isSelected: boolean;
}

export type ThoughtTreeFlowNodeData = UserFlowNodeData | AgentFlowNodeData;

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
