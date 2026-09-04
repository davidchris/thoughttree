import { describe, expect, it } from 'vitest';
import { GraphMutations } from './mutations';
import { normalizeProvenance, safeToolTitle } from './normalize';
import { GraphSerialize } from './serialize';
import type { GraphJSON, GraphNode, ToolActivityKind } from './types';
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

describe('GraphSerialize provenance normalization', () => {
  const forbiddenPattern = /raw(Input|Output|Payload)|commandText|payload|\/Users\/|secret contents|token|file:|\$TOKEN/;

  function adversarialJSON(): GraphJSON {
    return JSON.parse(JSON.stringify({
      version: 4,
      nodes: [
        {
          id: 'q',
          role: 'user',
          content: 'question',
          timestamp: 1,
          provenance: { completeness: 'complete', references: [], activity: [] },
          rawPayload: { token: 'secret' },
        },
        {
          id: 'a',
          role: 'assistant',
          content: 'answer',
          timestamp: 2,
          model: 'm',
          incomplete: true,
          rawOutput: 'secret contents',
          provenance: {
            completeness: 'partial',
            rawPayload: { token: 'secret' },
            references: [
              {
                type: 'url',
                url: 'https://example.com/a',
                title: 'A',
                relations: ['cited', 'bogus'],
                rawPayload: { token: 'secret' },
              },
              {
                type: 'file',
                scope: 'vault',
                path: '/Users/alice/secret.txt',
                displayName: 'secret.txt',
                relations: ['read'],
                rawPayload: { token: 'secret' },
              },
              {
                type: 'file',
                scope: 'vault',
                path: '../../Users/alice/secret.txt',
                displayName: 'secret.txt',
                relations: ['read'],
              },
              {
                type: 'file',
                scope: 'vault',
                path: 'notes/ok.md',
                displayName: 'ok.md',
                relations: ['read'],
              },
              {
                type: 'file',
                scope: 'external',
                displayName: 'outside.txt',
                path: '/Users/alice/outside.txt',
                relations: ['read'],
              },
              {
                type: 'file',
                scope: 'external',
                displayName: '/Users/alice/Documents/report.pdf',
                relations: ['read'],
              },
              { type: 'url', url: 'file:///Users/alice/secret.txt', title: 'Local', relations: ['consulted'] },
              { type: 'url', url: '/Users/alice/secret.txt', relations: ['consulted'] },
              {
                type: 'url',
                url: 'https://example.com/b',
                title: 'Copy at /Users/alice/secret.txt',
                domain: 'example.com',
                relations: ['fetched'],
              },
              { type: 'mystery', payload: { token: 'secret' } },
            ],
            activity: [
              {
                type: 'tool',
                kind: 'execute',
                title: 'Run command',
                status: 'completed',
                rawInput: 'cat /Users/alice/secret.txt',
                rawOutput: 'secret contents',
                commandText: 'cat /Users/alice/secret.txt',
              },
              { type: 'tool', kind: 'teleport', title: 'x'.repeat(250), status: 'done' },
              { type: 'tool', kind: 'execute', title: 'cat /Users/alice/secret.txt', status: 'completed' },
              { type: 'tool', kind: 'execute', title: 'echo $TOKEN | pbcopy', status: 'completed' },
              { type: 'tool', kind: 'read', title: 'Read  src/App.tsx', status: 'completed' },
              { type: 'commentary', content: 'Checking.', rawPayload: { token: 'secret' } },
              { type: 'unknown', providerType: 'x', label: 'x', payload: { token: 'secret' } },
              { type: 'mystery', payload: { token: 'secret' } },
            ],
          },
        },
      ],
      edges: [{ id: 'q->a', source: 'q', target: 'a' }],
      layout: [
        { id: 'q', position: { x: 0, y: 0 } },
        { id: 'a', position: { x: 0, y: 1 } },
      ],
    }));
  }

  const expectedAssistant: GraphNode = {
    id: 'a',
    role: 'assistant',
    content: 'answer',
    timestamp: 2,
    model: 'm',
    incomplete: true,
    provenance: {
      completeness: 'partial',
      references: [
        { type: 'url', url: 'https://example.com/a', title: 'A', relations: ['cited'] },
        { type: 'file', scope: 'external', displayName: 'secret.txt', relations: ['read'] },
        { type: 'file', scope: 'external', displayName: 'secret.txt', relations: ['read'] },
        { type: 'file', scope: 'vault', path: 'notes/ok.md', displayName: 'ok.md', relations: ['read'] },
        { type: 'file', scope: 'external', displayName: 'outside.txt', relations: ['read'] },
        { type: 'file', scope: 'external', displayName: 'report.pdf', relations: ['read'] },
        { type: 'url', url: 'https://example.com/b', domain: 'example.com', relations: ['fetched'] },
      ],
      activity: [
        { type: 'tool', kind: 'execute', title: 'Run command', status: 'completed' },
        { type: 'tool', kind: 'other', title: 'x'.repeat(200), titleTruncated: true, status: 'incomplete' },
        { type: 'tool', kind: 'execute', title: 'Ran a command', titleRedacted: true, status: 'completed' },
        { type: 'tool', kind: 'execute', title: 'Ran a command', titleRedacted: true, status: 'completed' },
        { type: 'tool', kind: 'read', title: 'Read src/App.tsx', status: 'completed' },
        { type: 'commentary', content: 'Checking.' },
        { type: 'unknown', providerType: 'x', label: 'x' },
      ],
    },
  };

  it('rebuilds nodes from an allowlist when loading', () => {
    const restored = GraphSerialize.fromJSON(adversarialJSON());

    expect(restored.nodes.get('q')).toEqual({ id: 'q', role: 'user', content: 'question', timestamp: 1 });
    expect(restored.nodes.get('a')).toEqual(expectedAssistant);
    expect(JSON.stringify(GraphSerialize.toJSON(restored))).not.toMatch(forbiddenPattern);
  });

  it('rebuilds nodes from an allowlist when saving in-memory nodes', () => {
    const json = adversarialJSON();
    const graph = {
      nodes: new Map(json.nodes.map((node) => [node.id, node])),
      edges: json.edges,
      layout: new Map(json.layout.map((entry) => [entry.id, entry.position])),
    };

    const serialized = GraphSerialize.toJSON(graph);

    expect(serialized.nodes).toEqual([
      { id: 'q', role: 'user', content: 'question', timestamp: 1 },
      expectedAssistant,
    ]);
    expect(JSON.stringify(serialized)).not.toMatch(forbiddenPattern);
  });

  it('normalizes legacy v2 node data through the same allowlist', () => {
    const g = GraphSerialize.fromLegacyV2({
      version: 2,
      nodes: [{ id: 'a', position: { x: 0, y: 0 } }],
      edges: [],
      nodeData: {
        a: {
          id: 'a',
          role: 'assistant' as const,
          content: 'r',
          timestamp: 2,
          rawOutput: 'secret',
        } as unknown as GraphNode,
      },
    });

    expect(g.nodes.get('a')).toEqual({ id: 'a', role: 'assistant', content: 'r', timestamp: 2 });
  });
});

describe('normalizeProvenance completeness', () => {
  const okReference = { type: 'url', url: 'https://example.com', relations: ['cited'] };
  const okTool = { type: 'tool', kind: 'read', title: 'Read notes.md', status: 'completed' };

  it('keeps a complete claim when nothing is dropped or reduced', () => {
    const provenance = normalizeProvenance({
      completeness: 'complete',
      references: [okReference],
      activity: [okTool, { type: 'commentary', content: 'Checking.' }],
    });
    expect(provenance?.completeness).toBe('complete');
  });

  it.each([
    ['an unsupported activity entry', { activity: [{ type: 'provider_status', label: 'x' }] }],
    ['an unsupported reference entry', { references: [{ type: 'mystery' }] }],
    ['retained Unknown activity', { activity: [{ type: 'unknown', providerType: 'x', label: 'x' }] }],
    ['a redacted tool title', { activity: [{ ...okTool, title: 'cat /Users/alice/secret.txt' }] }],
    ['a truncated tool title', { activity: [{ ...okTool, title: 'x'.repeat(201) }] }],
    ['a dropped file: URL', { references: [{ type: 'url', url: 'file:///Users/alice/x', relations: [] }] }],
    ['a Vault path that escapes the Vault', {
      references: [{ type: 'file', scope: 'vault', path: '../x', displayName: 'x', relations: [] }],
    }],
    ['a display name reduced to its basename', {
      references: [{ type: 'file', scope: 'external', displayName: '/Users/alice/x', relations: [] }],
    }],
    ['an unknown relation', { references: [{ ...okReference, relations: ['cited', 'bogus'] }] }],
  ])('downgrades complete to partial after %s', (_label, overrides) => {
    const provenance = normalizeProvenance({ completeness: 'complete', references: [], activity: [], ...overrides });
    expect(provenance?.completeness).toBe('partial');
  });

  it('never upgrades unknown even when nothing is lost', () => {
    const provenance = normalizeProvenance({ completeness: 'unknown', references: [okReference], activity: [] });
    expect(provenance?.completeness).toBe('unknown');
  });

  it('keeps unknown rather than claiming partial when evidence is dropped', () => {
    const provenance = normalizeProvenance({ completeness: 'unknown', references: [{ type: 'mystery' }], activity: [] });
    expect(provenance?.completeness).toBe('unknown');
  });
});

describe('safeToolTitle', () => {
  it.each([
    ['cat /Users/alice/secret.txt', 'execute', 'Ran a command'],
    ['Read ~/notes.md', 'read', 'Read a file'],
    ['Edit C:\\Users\\alice\\x.txt', 'edit', 'Edited a file'],
    ['ls | grep x', 'execute', 'Ran a command'],
    ['echo `whoami`', 'execute', 'Ran a command'],
    ['rm -rf a && rm b', 'delete', 'Deleted a file'],
    ['Search for=/etc/passwd', 'search', 'Ran a search'],
    ['   ', 'other', 'Used a tool'],
  ])('replaces %j with a generic %s summary', (title, kind, expected) => {
    expect(safeToolTitle(title, kind as ToolActivityKind)).toEqual({ title: expected, redacted: true });
  });

  it.each([
    ['Read src/App.tsx', 'Read src/App.tsx'],
    ['Run  tests\n(vitest)', 'Run tests (vitest)'],
    ['Search for "provenance"', 'Search for "provenance"'],
    ['Fetch https://example.com/docs', 'Fetch https://example.com/docs'],
  ])('keeps %j as a summary', (title, expected) => {
    expect(safeToolTitle(title, 'other')).toEqual({ title: expected, redacted: false });
  });
});
