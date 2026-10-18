import { describe, expect, it } from 'vitest';
import { GraphMutations, type GraphNode } from '@thoughttree/graph-model';
import { graphToFlowEdges, graphToFlowNodes } from './projection';

function userNode(id: string, ts = 1): GraphNode {
  return { id, role: 'user', content: '', timestamp: ts, contentUpdatedAt: ts };
}

function agentNode(id: string, ts = 1): GraphNode {
  return {
    id,
    role: 'assistant',
    content: '',
    timestamp: ts,
    contentUpdatedAt: ts,
    provider: 'claude-code',
  };
}

describe('graphToFlowNodes', () => {
  it('maps role to ReactFlow type and embeds id-only data', () => {
    let g = GraphMutations.empty();
    g = GraphMutations.addNode(g, userNode('a'), { x: 10, y: 20 });
    g = GraphMutations.addNode(g, agentNode('b'), { x: 10, y: 120 });

    const nodes = graphToFlowNodes(g, { selectedNodeId: null });
    expect(nodes).toHaveLength(2);

    const a = nodes.find((n) => n.id === 'a')!;
    expect(a.type).toBe('user');
    expect(a.position).toEqual({ x: 10, y: 20 });
    expect(a.data).toEqual({ id: 'a', isSelected: false });
    expect(a.dragHandle).toBe('.thought-node');

    const b = nodes.find((n) => n.id === 'b')!;
    expect(b.type).toBe('agent');
    expect(b.data).toEqual({ id: 'b', isSelected: false });
  });

  it('projects a file node to the file ReactFlow type', () => {
    const file: GraphNode = {
      id: 'f',
      role: 'file',
      content: '',
      timestamp: 1,
      path: 'notes/a.md',
      name: 'a.md',
      mimeType: 'text/markdown',
      size: 1,
      seenMtime: 1,
      seenSize: 1,
    };
    let g = GraphMutations.empty();
    g = GraphMutations.addNode(g, file, { x: 5, y: 6 });

    const [node] = graphToFlowNodes(g, { selectedNodeId: null });
    expect(node.type).toBe('file');
    expect(node.data).toEqual({ id: 'f', isSelected: false });
  });

  it('marks the selected node', () => {
    let g = GraphMutations.empty();
    g = GraphMutations.addNode(g, userNode('a'), { x: 0, y: 0 });
    g = GraphMutations.addNode(g, userNode('b'), { x: 0, y: 0 });
    const nodes = graphToFlowNodes(g, { selectedNodeId: 'b' });
    const a = nodes.find((n) => n.id === 'a')!;
    const b = nodes.find((n) => n.id === 'b')!;
    expect(a.data.isSelected).toBe(false);
    expect(b.data.isSelected).toBe(true);
    expect(a.selected).toBe(false);
    expect(b.selected).toBe(true);
  });

  it('falls back to origin when layout entry is missing', () => {
    const a = userNode('a');
    const g = { nodes: new Map([[a.id, a]]), edges: [], layout: new Map() };
    const nodes = graphToFlowNodes(g, { selectedNodeId: null });
    expect(nodes[0].position).toEqual({ x: 0, y: 0 });
  });
});

/**
 * Merge graph:  a   x
 *               |   |
 *               b   y
 *                \ /
 *                 m -- c (sibling branch off b: b -> c)
 */
function mergeGraph() {
  let g = GraphMutations.empty();
  for (const id of ['a', 'b', 'c', 'm', 'x', 'y']) g = GraphMutations.addNode(g, userNode(id), { x: 0, y: 0 });
  for (const [s, t] of [['a', 'b'], ['b', 'm'], ['b', 'c'], ['x', 'y'], ['y', 'm']]) g = GraphMutations.addEdge(g, s, t);
  return g;
}

describe('lineage highlight', () => {
  it('marks every ancestor of the selected node, across all parents of a merge', () => {
    const nodes = graphToFlowNodes(mergeGraph(), { selectedNodeId: 'm' });
    const lineage = nodes.filter((n) => n.className === 'lineage').map((n) => n.id).sort();
    expect(lineage).toEqual(['a', 'b', 'x', 'y']);
  });

  it('does not mark the selected node itself, descendants, or side branches', () => {
    const nodes = graphToFlowNodes(mergeGraph(), { selectedNodeId: 'b' });
    const byId = new Map(nodes.map((n) => [n.id, n]));
    expect(byId.get('a')!.className).toBe('lineage');
    for (const id of ['b', 'c', 'm', 'x', 'y']) expect(byId.get(id)!.className).toBeUndefined();
  });

  it('marks only the edges that lead into the selected node', () => {
    const edges = graphToFlowEdges(mergeGraph(), { selectedNodeId: 'm' });
    const lineage = edges.filter((e) => e.className === 'lineage').map((e) => e.id).sort();
    expect(lineage).toEqual(['a->b', 'b->m', 'x->y', 'y->m']);
  });

  it('marks nothing without a selection', () => {
    expect(graphToFlowNodes(mergeGraph(), { selectedNodeId: null }).some((n) => n.className)).toBe(false);
    expect(graphToFlowEdges(mergeGraph(), { selectedNodeId: null }).some((e) => e.className)).toBe(false);
  });
});

describe('graphToFlowEdges', () => {
  it('projects edges as floating edges', () => {
    let g = GraphMutations.empty();
    g = GraphMutations.addNode(g, userNode('a'), { x: 0, y: 0 });
    g = GraphMutations.addNode(g, userNode('b'), { x: 0, y: 0 });
    g = GraphMutations.addEdge(g, 'a', 'b');
    expect(graphToFlowEdges(g, { selectedNodeId: null })).toEqual([
      { id: 'a->b', source: 'a', target: 'b', type: 'floating' },
    ]);
  });
});
