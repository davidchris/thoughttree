export type NodeId = string;
export type GraphAgentProvider = 'claude-code' | 'gemini-cli' | 'codex';

export interface ImageAttachment {
  data: string;
  mimeType: string;
  name?: string;
}

export type ProvenanceCompleteness = 'complete' | 'partial' | 'unknown';

export type TurnReferenceRelation =
  | 'consulted'
  | 'cited'
  | 'read'
  | 'created'
  | 'updated'
  | 'deleted'
  | 'moved'
  | 'searched'
  | 'fetched';

interface TurnReferenceBase {
  relations: TurnReferenceRelation[];
  timestamp?: number;
}

export interface UrlTurnReference extends TurnReferenceBase {
  type: 'url';
  url: string;
  title?: string;
  domain?: string;
  index?: number;
  percentage?: number;
  is_search_result?: boolean;
}

export interface VaultFileTurnReference extends TurnReferenceBase {
  type: 'file';
  scope: 'vault';
  path: string;
  displayName: string;
}

export interface ExternalFileTurnReference extends TurnReferenceBase {
  type: 'file';
  scope: 'external';
  displayName: string;
}

export type FileTurnReference = VaultFileTurnReference | ExternalFileTurnReference;
export type TurnReference = UrlTurnReference | FileTurnReference;

interface TurnActivityBase {
  timestamp?: number;
}

export interface AssistantCommentary extends TurnActivityBase {
  type: 'commentary';
  content: string;
}

export type ToolActivityKind =
  | 'read'
  | 'edit'
  | 'delete'
  | 'move'
  | 'search'
  | 'execute'
  | 'fetch'
  | 'delegate'
  | 'other';

export type ToolActivityStatus = 'pending' | 'completed' | 'failed' | 'incomplete';

export interface ToolActivity extends TurnActivityBase {
  type: 'tool';
  kind: ToolActivityKind;
  title: string;
  titleTruncated?: boolean;
  /** True when the persisted title was replaced by a generic summary because it looked like a raw command or host path. */
  titleRedacted?: boolean;
  status: ToolActivityStatus;
  completedAt?: number;
}

export interface UnknownActivity extends TurnActivityBase {
  type: 'unknown';
  providerType: string;
  label: string;
}

export type TurnActivity = AssistantCommentary | ToolActivity | UnknownActivity;

export interface TurnProvenance {
  completeness: ProvenanceCompleteness;
  references: TurnReference[];
  activity: TurnActivity[];
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
  /** True when the Turn ended without an assistant answer (e.g. an unanswered trailing user message). */
  incomplete?: boolean;
  provenance?: TurnProvenance;
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
