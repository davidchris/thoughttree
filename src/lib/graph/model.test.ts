import { describe, expect, it } from 'vitest';
import { GraphModel } from './model';
import type { Graph, GraphEdge, GraphNode } from './types';

function userNode(id: string, content: string, ts: number): GraphNode {
  return { id, role: 'user', content, timestamp: ts, contentUpdatedAt: ts };
}

function agentNode(id: string, content: string, ts: number): GraphNode {
  return {
    id,
    role: 'assistant',
    content,
    timestamp: ts,
    contentUpdatedAt: ts,
    provider: 'claude-code',
  };
}

function edge(source: string, target: string): GraphEdge {
  return { id: `${source}->${target}`, source, target };
}

function graphOf(nodes: GraphNode[], edges: GraphEdge[]): Graph {
  return {
    nodes: new Map(nodes.map((n) => [n.id, n])),
    edges,
    layout: new Map(),
  };
}

describe('GraphModel.parents / children', () => {
  it('returns empty arrays on isolated node', () => {
    const a = userNode('a', '', 1);
    const g = graphOf([a], []);
    expect(GraphModel.parents(g, 'a')).toEqual([]);
    expect(GraphModel.children(g, 'a')).toEqual([]);
  });

  it('returns multiple parents for synthesizer node', () => {
    const a = userNode('a', '', 1);
    const b = userNode('b', '', 2);
    const c = userNode('c', '', 3);
    const g = graphOf([a, b, c], [edge('a', 'c'), edge('b', 'c')]);
    expect(GraphModel.parents(g, 'c').sort()).toEqual(['a', 'b']);
  });

  it('returns multiple children for forking node', () => {
    const a = userNode('a', '', 1);
    const b = userNode('b', '', 2);
    const c = userNode('c', '', 3);
    const g = graphOf([a, b, c], [edge('a', 'b'), edge('a', 'c')]);
    expect(GraphModel.children(g, 'a').sort()).toEqual(['b', 'c']);
  });
});

describe('GraphModel.ancestors / descendants', () => {
  it('walks linear chain', () => {
    const a = userNode('a', '', 1);
    const b = agentNode('b', '', 2);
    const c = userNode('c', '', 3);
    const g = graphOf([a, b, c], [edge('a', 'b'), edge('b', 'c')]);
    expect(GraphModel.ancestors(g, 'c')).toEqual(new Set(['a', 'b']));
    expect(GraphModel.descendants(g, 'a')).toEqual(new Set(['b', 'c']));
  });

  it('walks all parent paths in DAG (multi-parent)', () => {
    // a → c, b → c, c → d
    const a = userNode('a', '', 1);
    const b = userNode('b', '', 2);
    const c = userNode('c', '', 3);
    const d = userNode('d', '', 4);
    const g = graphOf([a, b, c, d], [edge('a', 'c'), edge('b', 'c'), edge('c', 'd')]);
    expect(GraphModel.ancestors(g, 'd')).toEqual(new Set(['a', 'b', 'c']));
  });

  it('handles diamond DAG without duplication', () => {
    // a → b, a → c, b → d, c → d
    const a = userNode('a', '', 1);
    const b = userNode('b', '', 2);
    const c = userNode('c', '', 3);
    const d = userNode('d', '', 4);
    const g = graphOf(
      [a, b, c, d],
      [edge('a', 'b'), edge('a', 'c'), edge('b', 'd'), edge('c', 'd')],
    );
    expect(GraphModel.ancestors(g, 'd')).toEqual(new Set(['a', 'b', 'c']));
    expect(GraphModel.descendants(g, 'a')).toEqual(new Set(['b', 'c', 'd']));
  });

  it('returns empty set on isolated node', () => {
    const a = userNode('a', '', 1);
    const g = graphOf([a], []);
    expect(GraphModel.ancestors(g, 'a')).toEqual(new Set());
    expect(GraphModel.descendants(g, 'a')).toEqual(new Set());
  });

  it('terminates when graph contains a cycle', () => {
    const a = userNode('a', '', 1);
    const b = userNode('b', '', 2);
    const g = graphOf([a, b], [edge('a', 'b'), edge('b', 'a')]);
    expect(GraphModel.ancestors(g, 'b')).toEqual(new Set(['a', 'b']));
    expect(GraphModel.descendants(g, 'a')).toEqual(new Set(['a', 'b']));
  });
});

describe('GraphModel.conversationPathIds', () => {
  it('returns [target] for isolated node', () => {
    const a = userNode('a', '', 1);
    const g = graphOf([a], []);
    expect(GraphModel.conversationPathIds(g, 'a')).toEqual(['a']);
  });

  it('orders linear chain by timestamp ascending, target last', () => {
    const a = userNode('a', '', 1);
    const b = agentNode('b', '', 2);
    const c = userNode('c', '', 3);
    const g = graphOf([a, b, c], [edge('a', 'b'), edge('b', 'c')]);
    expect(GraphModel.conversationPathIds(g, 'c')).toEqual(['a', 'b', 'c']);
  });

  it('falls back to timestamp order when ancestor subgraph contains a cycle', () => {
    // a → b, b → a — cycle. Target b: ancestors include both a and b.
    const a = userNode('a', '', 1);
    const b = userNode('b', '', 2);
    const g = graphOf([a, b], [edge('a', 'b'), edge('b', 'a')]);
    expect(GraphModel.conversationPathIds(g, 'b')).toEqual(['a', 'b']);
  });

  it('topo-sorts synthesizer ancestors by timestamp, dedupes shared ancestors', () => {
    // root → a, root → b, a → synth, b → synth   (timestamps in creation order)
    const root = userNode('root', '', 1);
    const a = agentNode('a', '', 2);
    const b = agentNode('b', '', 3);
    const synth = userNode('synth', '', 4);
    const g = graphOf(
      [root, a, b, synth],
      [edge('root', 'a'), edge('root', 'b'), edge('a', 'synth'), edge('b', 'synth')],
    );
    expect(GraphModel.conversationPathIds(g, 'synth')).toEqual(['root', 'a', 'b', 'synth']);
  });
});

describe('GraphModel.conversationPath', () => {
  it('emits ordered messages for linear chain', () => {
    const a = userNode('a', 'hello', 1);
    const b = agentNode('b', 'hi', 2);
    const c = userNode('c', 'follow up', 3);
    const g = graphOf([a, b, c], [edge('a', 'b'), edge('b', 'c')]);
    expect(GraphModel.conversationPath(g, 'c')).toEqual([
      { role: 'user', content: 'hello' },
      { role: 'assistant', content: 'hi' },
      { role: 'user', content: 'follow up' },
    ]);
  });

  it('skips empty-content nodes and merges remaining same-role neighbours', () => {
    // Empty agent in the middle gets dropped; surrounding user nodes merge.
    const a = userNode('a', 'first', 1);
    const b = agentNode('b', '   ', 2);
    const c = userNode('c', 'last', 3);
    const g = graphOf([a, b, c], [edge('a', 'b'), edge('b', 'c')]);
    expect(GraphModel.conversationPath(g, 'c')).toEqual([
      { role: 'user', content: 'first\n\nlast' },
    ]);
  });

  it('merges consecutive same-role messages by concatenating content', () => {
    // Two user parents converge into a synthesizer user node.
    // After topo-sort: user(a) → user(b) → user(synth) — three consecutive user roles.
    const a = userNode('a', 'one', 1);
    const b = userNode('b', 'two', 2);
    const synth = userNode('synth', 'three', 3);
    const g = graphOf([a, b, synth], [edge('a', 'synth'), edge('b', 'synth')]);
    expect(GraphModel.conversationPath(g, 'synth')).toEqual([
      { role: 'user', content: 'one\n\ntwo\n\nthree' },
    ]);
  });

  it('preserves alternation when roles already alternate', () => {
    const a = userNode('a', 'q1', 1);
    const b = agentNode('b', 'r1', 2);
    const c = userNode('c', 'q2', 3);
    const d = agentNode('d', 'r2', 4);
    const g = graphOf(
      [a, b, c, d],
      [edge('a', 'b'), edge('b', 'c'), edge('c', 'd')],
    );
    expect(GraphModel.conversationPath(g, 'd')).toEqual([
      { role: 'user', content: 'q1' },
      { role: 'assistant', content: 'r1' },
      { role: 'user', content: 'q2' },
      { role: 'assistant', content: 'r2' },
    ]);
  });

  it('includes images on user messages', () => {
    const img = { data: 'AAAA', mimeType: 'image/png' };
    const a: GraphNode = {
      id: 'a',
      role: 'user',
      content: 'look',
      timestamp: 1,
      contentUpdatedAt: 1,
      images: [img],
    };
    const g = graphOf([a], []);
    expect(GraphModel.conversationPath(g, 'a')).toEqual([
      { role: 'user', content: 'look', images: [img] },
    ]);
  });

  it('merges images when consecutive user messages are concatenated', () => {
    const imgA = { data: 'A', mimeType: 'image/png' };
    const imgB = { data: 'B', mimeType: 'image/png' };
    const a: GraphNode = {
      id: 'a',
      role: 'user',
      content: 'one',
      timestamp: 1,
      contentUpdatedAt: 1,
      images: [imgA],
    };
    const b: GraphNode = {
      id: 'b',
      role: 'user',
      content: 'two',
      timestamp: 2,
      contentUpdatedAt: 2,
      images: [imgB],
    };
    const g = graphOf([a, b], [edge('a', 'b')]);
    expect(GraphModel.conversationPath(g, 'b')).toEqual([
      { role: 'user', content: 'one\n\ntwo', images: [imgA, imgB] },
    ]);
  });
});
