import { normalizeGraphNode } from './normalize';
import type { Graph, GraphEdge, GraphJSON, GraphNode, NodeId, Position } from './types';

export const GRAPH_JSON_VERSION = 5;

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

function normalizedNodes(nodes: Iterable<unknown>): GraphNode[] {
  const result: GraphNode[] = [];
  for (const node of nodes) {
    const normalized = normalizeGraphNode(node);
    if (normalized) result.push(normalized);
  }
  return result;
}

/**
 * File nodes never have incoming edges (they are never Synthesizer nodes), so
 * an edge into one is dropped on load whatever the Project file claims.
 */
function edgesRespectingFileNodes(nodes: Map<NodeId, GraphNode>, edges: GraphEdge[]): GraphEdge[] {
  return edges.filter((edge) => nodes.get(edge.target)?.role !== 'file');
}

/**
 * Nodes are rebuilt from an explicit allowlist on both save and load so raw
 * tool data, unknown payloads, absolute paths, or user-node provenance never
 * reach a Project file, whatever a provider or an untrusted file supplied.
 */
export const GraphSerialize = {
  toJSON(g: Graph): GraphJSON {
    return {
      version: GRAPH_JSON_VERSION,
      nodes: normalizedNodes(g.nodes.values()),
      edges: g.edges.slice(),
      layout: Array.from(g.layout.entries()).map(([id, position]) => ({ id, position })),
    };
  },

  fromJSON(json: GraphJSON): Graph {
    const nodes = new Map(normalizedNodes(json.nodes).map((n) => [n.id, n]));
    return {
      nodes,
      edges: edgesRespectingFileNodes(nodes, json.edges),
      layout: new Map(json.layout.map((entry) => [entry.id, entry.position])),
    };
  },

  fromLegacyV2(legacy: LegacyV2ProjectFile): Graph {
    const nodes = new Map<NodeId, GraphNode>();
    const layout = new Map<NodeId, Position>();

    for (const flow of legacy.nodes) {
      const data = normalizeGraphNode(legacy.nodeData[flow.id]);
      if (!data) continue;
      nodes.set(flow.id, data);
      layout.set(flow.id, flow.position);
    }

    const edges: GraphEdge[] = legacy.edges.map((e) => ({
      id: e.id,
      source: e.source,
      target: e.target,
    }));

    return { nodes, edges: edgesRespectingFileNodes(nodes, edges), layout };
  },
};
