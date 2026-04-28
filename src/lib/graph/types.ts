import type { MessageNodeData } from '../../types';

export type NodeId = string;

export type GraphNode = MessageNodeData;

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
