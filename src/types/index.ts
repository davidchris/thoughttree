// Node data types - discriminated union for user vs agent nodes
export interface UserNodeData {
  id: string;
  role: 'user';
  content: string;
  timestamp: number;
}

export interface AgentNodeData {
  id: string;
  role: 'assistant';
  content: string;
  timestamp: number;
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
