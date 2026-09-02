import { describe, expect, it } from 'vitest';
import { GraphMutations } from './mutations';
import { GraphSerialize } from './serialize';
import type { GraphJSON, GraphNode } from './types';
import projectV4 from '../../../test/fixtures/project-v4.json';

function userNode(id: string, content = '', ts = 1): GraphNode {
  return { id, role: 'user', content, timestamp: ts, contentUpdatedAt: ts };
}

describe('GraphSerialize.toJSON / fromJSON', () => {
  it('round-trips an empty graph', () => {
    const g = GraphMutations.empty();
    const json = GraphSerialize.toJSON(g);
    const restored = GraphSerialize.fromJSON(json);
    expect(restored.nodes.size).toBe(0);
    expect(restored.edges).toEqual([]);
    expect(restored.layout.size).toBe(0);
  });

  it('round-trips nodes, edges, and layout', () => {
    let g = GraphMutations.empty();
    g = GraphMutations.addNode(g, userNode('a', 'hi', 1), { x: 10, y: 20 });
    g = GraphMutations.addNode(g, userNode('b', 'bye', 2), { x: 30, y: 40 });
    g = GraphMutations.addEdge(g, 'a', 'b');

    const json = GraphSerialize.toJSON(g);
    const restored = GraphSerialize.fromJSON(json);

    expect(restored.nodes.get('a')).toEqual(userNode('a', 'hi', 1));
    expect(restored.nodes.get('b')).toEqual(userNode('b', 'bye', 2));
    expect(restored.edges).toEqual([{ id: 'a->b', source: 'a', target: 'b' }]);
    expect(restored.layout.get('a')).toEqual({ x: 10, y: 20 });
    expect(restored.layout.get('b')).toEqual({ x: 30, y: 40 });
  });

  it('preserves ordered Turn provenance from the sanitized v4 fixture', () => {
    const graphJSON = projectV4.graph as GraphJSON;

    const restored = GraphSerialize.fromJSON(graphJSON);
    const serialized = GraphSerialize.toJSON(restored);

    expect(serialized).toEqual(graphJSON);
    expect(serialized.nodes[1]).toMatchObject({
      content: 'The exact assistant answer stays unchanged.',
      provenance: {
        completeness: 'partial',
        references: [
          { type: 'url', relations: ['consulted', 'cited'] },
          { type: 'file', scope: 'vault', relations: ['read', 'cited'] },
          { type: 'file', scope: 'external', relations: ['read'] },
        ],
        activity: [
          { type: 'commentary', content: 'I’m checking the cited evidence first.' },
          { type: 'tool', kind: 'read', titleTruncated: true },
          { type: 'commentary', content: 'The source and Vault file agree.' },
          { type: 'unknown', providerType: 'provider_status' },
        ],
      },
    });
  });
});

describe('GraphSerialize.fromLegacyV2', () => {
  it('reads the existing ProjectFile v2 shape', () => {
    const legacy = {
      version: 2,
      nodes: [
        { id: 'a', type: 'user', position: { x: 10, y: 20 }, data: {} },
        { id: 'b', type: 'agent', position: { x: 30, y: 40 }, data: {} },
      ],
      edges: [{ id: 'a-b', source: 'a', target: 'b' }],
      nodeData: {
        a: { id: 'a', role: 'user' as const, content: 'q', timestamp: 1 },
        b: { id: 'b', role: 'assistant' as const, content: 'r', timestamp: 2 },
      },
    };

    const g = GraphSerialize.fromLegacyV2(legacy);

    expect(g.nodes.get('a')).toEqual({ id: 'a', role: 'user', content: 'q', timestamp: 1 });
    expect(g.nodes.get('b')).toEqual({
      id: 'b',
      role: 'assistant',
      content: 'r',
      timestamp: 2,
    });
    expect(g.edges).toEqual([{ id: 'a-b', source: 'a', target: 'b' }]);
    expect(g.layout.get('a')).toEqual({ x: 10, y: 20 });
    expect(g.layout.get('b')).toEqual({ x: 30, y: 40 });
  });

  it('drops nodeData entries with no matching ReactFlow node', () => {
    const legacy = {
      version: 2,
      nodes: [{ id: 'a', type: 'user', position: { x: 0, y: 0 }, data: {} }],
      edges: [],
      nodeData: {
        a: { id: 'a', role: 'user' as const, content: '', timestamp: 1 },
        orphan: { id: 'orphan', role: 'user' as const, content: '', timestamp: 1 },
      },
    };
    const g = GraphSerialize.fromLegacyV2(legacy);
    expect(g.nodes.has('orphan')).toBe(false);
  });
});
