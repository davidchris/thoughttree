import type { Graph, GraphEdge, GraphJSON, GraphNode, NodeId, Position } from './types';

export const GRAPH_JSON_VERSION = 3;

interface LegacyV2Node {
  id: NodeId;
  position: Position;
  [key: string]: unknown;
}

interface LegacyV2Edge {
  id: string;
  source: NodeId;
  target: NodeId;
  [key: string]: unknown;
}

interface LegacyV2ProjectFile {
  version: number;
  nodes: LegacyV2Node[];
  edges: LegacyV2Edge[];
  nodeData: Record<NodeId, GraphNode>;
}

export const GraphSerialize = {
  toJSON(g: Graph): GraphJSON {
    return {
      version: GRAPH_JSON_VERSION,
      nodes: Array.from(g.nodes.values()),
      edges: g.edges.slice(),
      layout: Array.from(g.layout.entries()).map(([id, position]) => ({ id, position })),
    };
  },

  fromJSON(json: GraphJSON): Graph {
    return {
      nodes: new Map(json.nodes.map((n) => [n.id, n])),
      edges: json.edges.slice(),
      layout: new Map(json.layout.map((entry) => [entry.id, entry.position])),
    };
  },

  fromLegacyV2(legacy: LegacyV2ProjectFile): Graph {
    const nodes = new Map<NodeId, GraphNode>();
    const layout = new Map<NodeId, Position>();

    for (const flow of legacy.nodes) {
      const data = legacy.nodeData[flow.id];
      if (!data) continue;
      nodes.set(flow.id, data);
      layout.set(flow.id, flow.position);
    }

    const edges: GraphEdge[] = legacy.edges.map((e) => ({
      id: e.id,
      source: e.source,
      target: e.target,
    }));

    return { nodes, edges, layout };
  },
};
