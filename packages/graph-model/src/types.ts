export type NodeId = string;
export type GraphAgentProvider = 'claude-code' | 'gemini-cli' | 'codex';

export interface ImageAttachment {
  data: string;
  mimeType: string;
  name?: string;
}

export interface UserGraphNode {
  id: NodeId;
  role: 'user';
  content: string;
  timestamp: number;
  contentUpdatedAt?: number;
  summary?: string;
  summaryTimestamp?: number;
  images?: ImageAttachment[];
}

export interface AssistantGraphNode {
  id: NodeId;
  role: 'assistant';
  content: string;
  timestamp: number;
  contentUpdatedAt?: number;
  summary?: string;
  summaryTimestamp?: number;
  provider?: GraphAgentProvider;
  model?: string;
}

export type GraphNode = UserGraphNode | AssistantGraphNode;

export interface GraphEdge {
  id: string;
  source: NodeId;
  target: NodeId;
}

export interface Position {
  x: number;
  y: number;
}

export interface Graph {
  nodes: Map<NodeId, GraphNode>;
  edges: GraphEdge[];
  layout: Map<NodeId, Position>;
}

export interface GraphJSON {
  version: number;
  nodes: GraphNode[];
  edges: GraphEdge[];
  layout: Array<{ id: NodeId; position: Position }>;
}
