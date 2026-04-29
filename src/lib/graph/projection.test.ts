import { describe, expect, it } from 'vitest';
import { GraphMutations } from './mutations';
import { graphToFlowEdges, graphToFlowNodes } from './projection';
import type { GraphNode } from './types';

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

describe('graphToFlowEdges', () => {
  it('passes edges through unchanged', () => {
    let g = GraphMutations.empty();
    g = GraphMutations.addNode(g, userNode('a'), { x: 0, y: 0 });
    g = GraphMutations.addNode(g, userNode('b'), { x: 0, y: 0 });
    g = GraphMutations.addEdge(g, 'a', 'b');
    expect(graphToFlowEdges(g)).toEqual([{ id: 'a->b', source: 'a', target: 'b' }]);
  });
});
