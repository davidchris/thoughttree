import { describe, expect, it } from 'vitest';
import { GraphModel } from './model';
import { GraphMutations } from './mutations';
import type { GraphNode } from './types';

function userNode(id: string, content = '', ts = 1): GraphNode {
  return { id, role: 'user', content, timestamp: ts, contentUpdatedAt: ts };
}

describe('GraphMutations.empty', () => {
  it('creates an empty Graph', () => {
    const g = GraphMutations.empty();
    expect(g.nodes.size).toBe(0);
    expect(g.edges).toEqual([]);
    expect(g.layout.size).toBe(0);
  });
});

describe('GraphMutations.addNode', () => {
  it('inserts node and layout entry without mutating input', () => {
    const before = GraphMutations.empty();
    const a = userNode('a');
    const after = GraphMutations.addNode(before, a, { x: 10, y: 20 });
    expect(after.nodes.get('a')).toEqual(a);
    expect(after.layout.get('a')).toEqual({ x: 10, y: 20 });
    expect(before.nodes.size).toBe(0);
    expect(before.layout.size).toBe(0);
  });
});

describe('GraphMutations.addEdge', () => {
  it('appends edge with synthesized id', () => {
    const a = userNode('a', '', 1);
    const b = userNode('b', '', 2);
    let g = GraphMutations.empty();
    g = GraphMutations.addNode(g, a, { x: 0, y: 0 });
    g = GraphMutations.addNode(g, b, { x: 0, y: 100 });
    g = GraphMutations.addEdge(g, 'a', 'b');
    expect(g.edges).toEqual([{ id: 'a->b', source: 'a', target: 'b' }]);
  });
});

describe('GraphMutations.removeNode', () => {
  it('drops node, its layout entry, and any touching edges', () => {
    const a = userNode('a', '', 1);
    const b = userNode('b', '', 2);
    const c = userNode('c', '', 3);
    let g = GraphMutations.empty();
    g = GraphMutations.addNode(g, a, { x: 0, y: 0 });
    g = GraphMutations.addNode(g, b, { x: 0, y: 100 });
    g = GraphMutations.addNode(g, c, { x: 0, y: 200 });
    g = GraphMutations.addEdge(g, 'a', 'b');
    g = GraphMutations.addEdge(g, 'b', 'c');

    g = GraphMutations.removeNode(g, 'b');

    expect(g.nodes.has('b')).toBe(false);
    expect(g.layout.has('b')).toBe(false);
    expect(g.edges).toEqual([]);
    expect(GraphModel.parents(g, 'c')).toEqual([]);
  });
});

describe('GraphMutations.updateNode', () => {
  it('applies a patch and bumps contentUpdatedAt when content changes', () => {
    const a = userNode('a', 'old', 1);
    let g = GraphMutations.empty();
    g = GraphMutations.addNode(g, a, { x: 0, y: 0 });
    g = GraphMutations.updateNode(g, 'a', { content: 'new', contentUpdatedAt: 42 });
    expect(g.nodes.get('a')?.content).toBe('new');
    expect(g.nodes.get('a')?.contentUpdatedAt).toBe(42);
    expect(g.nodes.get('a')?.timestamp).toBe(1);
  });

  it('is a no-op for unknown ids', () => {
    const g = GraphMutations.empty();
    const same = GraphMutations.updateNode(g, 'missing', { content: 'x' });
    expect(same).toBe(g);
  });
});

describe('GraphMutations.appendContent', () => {
  it('concatenates a chunk to existing content', () => {
    const a = userNode('a', 'hello', 1);
    let g = GraphMutations.empty();
    g = GraphMutations.addNode(g, a, { x: 0, y: 0 });
    g = GraphMutations.appendContent(g, 'a', ' world', 99);
    expect(g.nodes.get('a')?.content).toBe('hello world');
    expect(g.nodes.get('a')?.contentUpdatedAt).toBe(99);
  });
});

describe('GraphMutations.setPosition', () => {
  it('updates layout entry only', () => {
    const a = userNode('a');
    let g = GraphMutations.empty();
    g = GraphMutations.addNode(g, a, { x: 0, y: 0 });
    g = GraphMutations.setPosition(g, 'a', { x: 50, y: 60 });
    expect(g.layout.get('a')).toEqual({ x: 50, y: 60 });
    expect(g.nodes.get('a')).toEqual(a);
  });
});
