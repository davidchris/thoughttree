import type { ImageAttachment } from '../../types';
import type { Graph, GraphEdge, NodeId } from './types';

interface Adjacency {
  parents: Map<NodeId, NodeId[]>;
  children: Map<NodeId, NodeId[]>;
}

const adjacencyCache = new WeakMap<GraphEdge[], Adjacency>();

function adjacency(edges: GraphEdge[]): Adjacency {
  const cached = adjacencyCache.get(edges);
  if (cached) return cached;
  const parents = new Map<NodeId, NodeId[]>();
  const children = new Map<NodeId, NodeId[]>();
  for (const e of edges) {
    const ps = parents.get(e.target);
    if (ps) ps.push(e.source);
    else parents.set(e.target, [e.source]);
    const cs = children.get(e.source);
    if (cs) cs.push(e.target);
    else children.set(e.source, [e.target]);
  }
  const built: Adjacency = { parents, children };
  adjacencyCache.set(edges, built);
  return built;
}

function bfs(start: NodeId, neighbours: Map<NodeId, NodeId[]>): Set<NodeId> {
  const result = new Set<NodeId>();
  const queue: NodeId[] = [...(neighbours.get(start) ?? [])];
  while (queue.length > 0) {
    const cur = queue.shift()!;
    if (result.has(cur)) continue;
    result.add(cur);
    const next = neighbours.get(cur);
    if (next) queue.push(...next);
  }
  return result;
}

interface ConversationMessage {
  role: string;
  content: string;
  images?: ImageAttachment[];
}

export const GraphModel = {
  parents(g: Graph, id: NodeId): NodeId[] {
    return adjacency(g.edges).parents.get(id) ?? [];
  },

  children(g: Graph, id: NodeId): NodeId[] {
    return adjacency(g.edges).children.get(id) ?? [];
  },

  ancestors(g: Graph, id: NodeId): Set<NodeId> {
    return bfs(id, adjacency(g.edges).parents);
  },

  descendants(g: Graph, id: NodeId): Set<NodeId> {
    return bfs(id, adjacency(g.edges).children);
  },

  conversationPathIds(g: Graph, targetId: NodeId): NodeId[] {
    const include = GraphModel.ancestors(g, targetId);
    include.add(targetId);

    const adj = adjacency(g.edges);
    const inDegree = new Map<NodeId, number>();
    for (const id of include) inDegree.set(id, 0);
    for (const id of include) {
      for (const parent of adj.parents.get(id) ?? []) {
        if (include.has(parent)) {
          inDegree.set(id, (inDegree.get(id) ?? 0) + 1);
        }
      }
    }

    const ready: NodeId[] = [];
    for (const [id, deg] of inDegree) {
      if (deg === 0) ready.push(id);
    }

    const result: NodeId[] = [];
    const emitted = new Set<NodeId>();
    const tsOf = (id: NodeId) => g.nodes.get(id)?.timestamp ?? 0;

    while (ready.length > 0) {
      ready.sort((a, b) => tsOf(a) - tsOf(b));
      const next = ready.shift()!;
      result.push(next);
      emitted.add(next);
      for (const child of adj.children.get(next) ?? []) {
        if (!include.has(child)) continue;
        const remaining = (inDegree.get(child) ?? 0) - 1;
        inDegree.set(child, remaining);
        if (remaining === 0) ready.push(child);
      }
    }

    // Cycle fallback: any include nodes left unemitted belong to a cycle.
    // Topological order is undefined for them; fall back to timestamp order
    // so the conversation path is non-empty rather than silently dropped.
    if (emitted.size < include.size) {
      const leftover: NodeId[] = [];
      for (const id of include) if (!emitted.has(id)) leftover.push(id);
      leftover.sort((a, b) => tsOf(a) - tsOf(b));
      result.push(...leftover);
    }

    return result;
  },

  conversationPath(g: Graph, targetId: NodeId): ConversationMessage[] {
    const ids = GraphModel.conversationPathIds(g, targetId);
    const merged: ConversationMessage[] = [];

    for (const id of ids) {
      const node = g.nodes.get(id);
      if (!node) continue;
      if (!node.content.trim()) continue;

      const last = merged[merged.length - 1];
      if (last && last.role === node.role) {
        last.content = `${last.content}\n\n${node.content}`;
        if (node.role === 'user' && node.images?.length) {
          last.images = [...(last.images ?? []), ...node.images];
        }
        continue;
      }

      const message: ConversationMessage = { role: node.role, content: node.content };
      if (node.role === 'user' && node.images?.length) {
        message.images = [...node.images];
      }
      merged.push(message);
    }

    return merged;
  },
};
