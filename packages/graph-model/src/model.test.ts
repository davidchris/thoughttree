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
    const a = userNode('a', '', 1);
    const b = userNode('b', '', 2);
    const c = userNode('c', '', 3);
    const d = userNode('d', '', 4);
    const g = graphOf([a, b, c, d], [edge('a', 'c'), edge('b', 'c'), edge('c', 'd')]);
    expect(GraphModel.ancestors(g, 'd')).toEqual(new Set(['a', 'b', 'c']));
  });

  it('handles diamond DAG without duplication', () => {
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
    const a = userNode('a', '', 1);
    const b = userNode('b', '', 2);
    const g = graphOf([a, b], [edge('a', 'b'), edge('b', 'a')]);
    expect(GraphModel.conversationPathIds(g, 'b')).toEqual(['a', 'b']);
  });

  it('topo-sorts synthesizer ancestors by timestamp, dedupes shared ancestors', () => {
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

describe('GraphModel.hasNonLinearLineage', () => {
  it('keeps the Structure gate closed for a linear Lineage subgraph', () => {
    const a = userNode('a', '', 1);
    const b = agentNode('b', '', 2);
    const c = userNode('c', '', 3);
    const g = graphOf([a, b, c], [edge('a', 'b'), edge('b', 'c')]);
    expect(GraphModel.hasNonLinearLineage(g, 'c')).toBe(false);
  });

  it('opens the Structure gate when the target has two parents in its Lineage subgraph', () => {
    const a = userNode('a', '', 1);
    const b = agentNode('b', '', 2);
    const c = userNode('c', '', 3);
    const g = graphOf([a, b, c], [edge('a', 'c'), edge('b', 'c')]);
    expect(GraphModel.hasNonLinearLineage(g, 'c')).toBe(true);
  });

  it('opens the Structure gate for a diamond Lineage subgraph', () => {
    const a = userNode('a', '', 1);
    const b = agentNode('b', '', 2);
    const c = userNode('c', '', 3);
    const d = userNode('d', '', 4);
    const g = graphOf(
      [a, b, c, d],
      [edge('a', 'b'), edge('a', 'c'), edge('b', 'd'), edge('c', 'd')],
    );
    expect(GraphModel.hasNonLinearLineage(g, 'd')).toBe(true);
  });

  it('keeps the Structure gate closed when a sibling branch is outside the Lineage subgraph', () => {
    const a = userNode('a', '', 1);
    const b = agentNode('b', '', 2);
    const c = userNode('c', '', 3);
    const d = userNode('d', '', 4);
    const g = graphOf(
      [a, b, c, d],
      [edge('a', 'b'), edge('a', 'c'), edge('b', 'd')],
    );
    expect(GraphModel.hasNonLinearLineage(g, 'd')).toBe(false);
  });
});

describe('GraphModel.conversationPath', () => {
  it('keeps linear Conversation path messages byte-identical with no graph structure', () => {
    const a = userNode('a', 'hello', 1);
    const b = agentNode('b', 'hi', 2);
    const c = userNode('c', 'follow up', 3);
    const g = graphOf([a, b, c], [edge('a', 'b'), edge('b', 'c')]);
    const path = GraphModel.conversationPath(g, 'c');
    expect(path).toEqual([
      { role: 'user', content: 'hello' },
      { role: 'assistant', content: 'hi' },
      { role: 'user', content: 'follow up' },
    ]);
    expect(path.map((m) => m.content).join('\n')).not.toContain('<node');
    expect(path.map((m) => m.content).join('\n')).not.toContain('<graph-map>');
  });

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

  it('wraps a non-linear Conversation path with Node markers, merge annotation, and Lineage map', () => {
    const a = agentNode('a', 'left branch', 1);
    const b = agentNode('b', 'right branch', 2);
    const synth = userNode('synth', 'synthesize them', 3);
    const g = graphOf([a, b, synth], [edge('a', 'synth'), edge('b', 'synth')]);
    expect(GraphModel.conversationPath(g, 'synth')).toEqual([
      {
        role: 'assistant',
        content: '<node id="a">\nleft branch\n</node>\n\n<node id="b">\nright branch\n</node>',
      },
      {
        role: 'user',
        content:
          '<node id="synt">\n' +
          '<graph: this message merges branches a, b>\n' +
          'synthesize them\n' +
          '</node>\n\n' +
          '<graph-map>\n' +
          "This conversation is a DAG, not a line: the messages above are a linearization of the current node's ancestor graph. <node id> markers tie each text segment to a graph node; the map below is the topology.\n" +
          'a (assistant) <- (root)\n' +
          'b (assistant) <- (root)\n' +
          'synt (user) <- a, b [current]\n' +
          '</graph-map>',
      },
    ]);
  });

  it('annotates user branch heads and maps both branches in a diamond Lineage subgraph', () => {
    const root = agentNode('root', 'base answer', 1);
    const left = userNode('left', 'follow left', 2);
    const righ = userNode('righ', 'follow right', 3);
    const done = agentNode('done', 'combined answer', 4);
    const g = graphOf(
      [root, left, righ, done],
      [edge('root', 'left'), edge('root', 'righ'), edge('left', 'done'), edge('righ', 'done')],
    );
    expect(GraphModel.conversationPath(g, 'done')).toEqual([
      {
        role: 'assistant',
        content: '<node id="root">\nbase answer\n</node>',
      },
      {
        role: 'user',
        content:
          '<node id="left">\n' +
          '<graph: this message starts a new branch from root>\n' +
          'follow left\n' +
          '</node>\n\n' +
          '<node id="righ">\n' +
          '<graph: this message starts a new branch from root>\n' +
          'follow right\n' +
          '</node>',
      },
      {
        role: 'assistant',
        content:
          '<node id="done">\ncombined answer\n</node>\n\n' +
          '<graph-map>\n' +
          "This conversation is a DAG, not a line: the messages above are a linearization of the current node's ancestor graph. <node id> markers tie each text segment to a graph node; the map below is the topology.\n" +
          'root (assistant) <- (root)\n' +
          'left (user) <- root\n' +
          'righ (user) <- root\n' +
          'done (assistant) <- left, righ [current]\n' +
          '</graph-map>',
      },
    ]);
  });

  it('merges consecutive same-role GraphNodes under an open Structure gate as complete Node markers', () => {
    const left = agentNode('left', 'left answer', 1);
    const righ = agentNode('righ', 'right answer', 2);
    const done = userNode('done', 'compare', 3);
    const g = graphOf([left, righ, done], [edge('left', 'done'), edge('righ', 'done')]);
    const [message] = GraphModel.conversationPath(g, 'done');
    expect(message).toEqual({
      role: 'assistant',
      content: '<node id="left">\nleft answer\n</node>\n\n<node id="righ">\nright answer\n</node>',
    });
  });

  it('extends Short ids when Lineage subgraph GraphNode ids share a four-character prefix', () => {
    const first = agentNode('abcd-left', 'first', 1);
    const second = agentNode('abcd-right', 'second', 2);
    const done = userNode('done', 'merge', 3);
    const g = graphOf(
      [first, second, done],
      [edge('abcd-left', 'done'), edge('abcd-right', 'done')],
    );
    const path = GraphModel.conversationPath(g, 'done');
    expect(path[0].content).toBe(
      '<node id="abcd">\nfirst\n</node>\n\n<node id="abcd-">\nsecond\n</node>',
    );
    expect(path[1].content).toContain('<graph: this message merges branches abcd, abcd->');
    expect(path[1].content).toContain('done (user) <- abcd, abcd- [current]');
  });

  it('omits empty-content GraphNodes from messages but keeps them in the Lineage map', () => {
    const left = agentNode('left', 'visible', 1);
    const empt = agentNode('empt', '   ', 2);
    const done = userNode('done', 'merge visible and empty', 3);
    const g = graphOf([left, empt, done], [edge('left', 'done'), edge('empt', 'done')]);
    const path = GraphModel.conversationPath(g, 'done');
    const content = path.map((message) => message.content).join('\n\n');
    expect(content).toContain('<node id="left">\nvisible\n</node>');
    expect(content).not.toContain('<node id="empt">');
    expect(content).toContain('empt (assistant) <- (root)');
    expect(content).toContain('done (user) <- left, empt [current]');
  });

  it('skips empty-content nodes and merges remaining same-role neighbours', () => {
    const a = userNode('a', 'first', 1);
    const b = agentNode('b', '   ', 2);
    const c = userNode('c', 'last', 3);
    const g = graphOf([a, b, c], [edge('a', 'b'), edge('b', 'c')]);
    expect(GraphModel.conversationPath(g, 'c')).toEqual([{ role: 'user', content: 'first\n\nlast' }]);
  });

  it('merges consecutive same-role messages by concatenating content', () => {
    const a = userNode('a', 'one', 1);
    const b = userNode('b', 'two', 2);
    const synth = userNode('synth', 'three', 3);
    const g = graphOf([a, b, synth], [edge('a', 'b'), edge('b', 'synth')]);
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
