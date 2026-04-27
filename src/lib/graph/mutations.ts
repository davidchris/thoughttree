import type { Graph, GraphNode, NodeId, Position } from './types';

function cloneGraph(g: Graph): Graph {
  return {
    nodes: new Map(g.nodes),
    edges: g.edges.slice(),
    layout: new Map(g.layout),
  };
}

export const GraphMutations = {
  empty(): Graph {
    return { nodes: new Map(), edges: [], layout: new Map() };
  },

  addNode(g: Graph, node: GraphNode, position: Position): Graph {
    const next = cloneGraph(g);
    next.nodes.set(node.id, node);
    next.layout.set(node.id, position);
    return next;
  },

  addEdge(g: Graph, source: NodeId, target: NodeId): Graph {
    const next = cloneGraph(g);
    next.edges = [...next.edges, { id: `${source}->${target}`, source, target }];
    return next;
  },

  removeNode(g: Graph, id: NodeId): Graph {
    if (!g.nodes.has(id)) return g;
    const next = cloneGraph(g);
    next.nodes.delete(id);
    next.layout.delete(id);
    next.edges = next.edges.filter((e) => e.source !== id && e.target !== id);
    return next;
  },

  updateNode(g: Graph, id: NodeId, patch: Partial<GraphNode>): Graph {
    const current = g.nodes.get(id);
    if (!current) return g;
    const next = cloneGraph(g);
    next.nodes.set(id, { ...current, ...patch } as GraphNode);
    return next;
  },

  appendContent(g: Graph, id: NodeId, chunk: string, contentUpdatedAt: number): Graph {
    const current = g.nodes.get(id);
    if (!current) return g;
    return GraphMutations.updateNode(g, id, {
      content: current.content + chunk,
      contentUpdatedAt,
    });
  },

  setPosition(g: Graph, id: NodeId, position: Position): Graph {
    const next = cloneGraph(g);
    next.layout.set(id, position);
    return next;
  },
};
