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

const LINEAGE_MAP_INTRO =
  "This conversation is a DAG, not a line: the messages above are a linearization of the current node's ancestor graph. <node id> markers tie each text segment to a graph node; the map below is the topology.";

function assignShortIds(ids: NodeId[]): Map<NodeId, string> {
  const used = new Set<string>();
  const shortIds = new Map<NodeId, string>();

  for (const id of ids) {
    let length = Math.min(id.length, 4);
    let shortId = id.slice(0, length);
    while (used.has(shortId) && length < id.length) {
      length += 1;
      shortId = id.slice(0, length);
    }
    used.add(shortId);
    shortIds.set(id, shortId);
  }

  return shortIds;
}

function lineageParents(
  id: NodeId,
  adj: Adjacency,
  include: Set<NodeId>,
  pathIndex: Map<NodeId, number>,
): NodeId[] {
  return (adj.parents.get(id) ?? [])
    .filter((parent) => include.has(parent))
    .sort((a, b) => (pathIndex.get(a) ?? 0) - (pathIndex.get(b) ?? 0));
}

function lineageChildren(
  id: NodeId,
  adj: Adjacency,
  include: Set<NodeId>,
  pathIndex: Map<NodeId, number>,
): NodeId[] {
  return (adj.children.get(id) ?? [])
    .filter((child) => include.has(child))
    .sort((a, b) => (pathIndex.get(a) ?? 0) - (pathIndex.get(b) ?? 0));
}

function userStructureAnnotations(
  id: NodeId,
  adj: Adjacency,
  include: Set<NodeId>,
  pathIndex: Map<NodeId, number>,
  shortIds: Map<NodeId, string>,
): string[] {
  const annotations: string[] = [];
  const parents = lineageParents(id, adj, include, pathIndex);
  if (parents.length >= 2) {
    annotations.push(
      `<graph: this message merges branches ${parents.map((parent) => shortIds.get(parent)).join(', ')}>`,
    );
  }

  const branchParent = parents.find(
    (parent) => lineageChildren(parent, adj, include, pathIndex).length >= 2,
  );
  if (branchParent) {
    annotations.push(`<graph: this message starts a new branch from ${shortIds.get(branchParent)}>`);
  }

  return annotations;
}

function nodeMarker(
  id: NodeId,
  role: string,
  content: string,
  adj: Adjacency,
  include: Set<NodeId>,
  pathIndex: Map<NodeId, number>,
  shortIds: Map<NodeId, string>,
): string {
  const annotations =
    role === 'user' ? userStructureAnnotations(id, adj, include, pathIndex, shortIds) : [];
  const annotationText = annotations.length > 0 ? `${annotations.join('\n')}\n` : '';
  return `<node id="${shortIds.get(id)}">\n${annotationText}${content}\n</node>`;
}

function lineageMap(
  ids: NodeId[],
  targetId: NodeId,
  g: Graph,
  adj: Adjacency,
  include: Set<NodeId>,
  pathIndex: Map<NodeId, number>,
  shortIds: Map<NodeId, string>,
): string {
  const lines = ids.map((id) => {
    const node = g.nodes.get(id);
    const parents = lineageParents(id, adj, include, pathIndex);
    const parentText =
      parents.length > 0 ? parents.map((parent) => shortIds.get(parent)).join(', ') : '(root)';
    const current = id === targetId ? ' [current]' : '';
    return `${shortIds.get(id)} (${node?.role ?? 'unknown'}) <- ${parentText}${current}`;
  });

  return `<graph-map>\n${LINEAGE_MAP_INTRO}\n${lines.join('\n')}\n</graph-map>`;
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

  hasNonLinearLineage(g: Graph, targetId: NodeId): boolean {
    const include = GraphModel.ancestors(g, targetId);
    include.add(targetId);

    const adj = adjacency(g.edges);
    for (const id of include) {
      const parentCount = (adj.parents.get(id) ?? []).filter((parent) => include.has(parent)).length;
      const childCount = (adj.children.get(id) ?? []).filter((child) => include.has(child)).length;
      if (parentCount > 1 || childCount > 1) return true;
    }

    return false;
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

    if (GraphModel.hasNonLinearLineage(g, targetId)) {
      const adj = adjacency(g.edges);
      const include = new Set(ids);
      const pathIndex = new Map(ids.map((id, index) => [id, index]));
      const shortIds = assignShortIds(ids);

      for (const id of ids) {
        const node = g.nodes.get(id);
        if (!node) continue;
        if (!node.content.trim()) continue;

        const content = nodeMarker(id, node.role, node.content, adj, include, pathIndex, shortIds);
        const last = merged[merged.length - 1];
        if (last && last.role === node.role) {
          last.content = `${last.content}\n\n${content}`;
          if (node.role === 'user' && node.images?.length) {
            last.images = [...(last.images ?? []), ...node.images];
          }
          continue;
        }

        const message: ConversationMessage = { role: node.role, content };
        if (node.role === 'user' && node.images?.length) {
          message.images = [...node.images];
        }
        merged.push(message);
      }

      if (merged.length > 0) {
        const final = merged[merged.length - 1];
        final.content = `${final.content}\n\n${lineageMap(ids, targetId, g, adj, include, pathIndex, shortIds)}`;
      }

      return merged;
    }

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
