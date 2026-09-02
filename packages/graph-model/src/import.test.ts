import { describe, expect, it } from 'vitest';
import { conversationToGraph } from './index';
import type { ImportedConversation } from './index';

describe('conversationToGraph', () => {
  it('builds a deterministic linear Graph without changing assistant Markdown', () => {
    const conversation: ImportedConversation = {
      importKey: 'session/alpha',
      turns: [
        {
          userMessage: 'Explain the result.',
          assistantAnswer: '# Result\n\n```ts\nconst exact = true;\n```\n',
          model: 'example-model',
          userTimestamp: 1_000,
          assistantTimestamp: 2_000,
        },
        {
          userMessage: 'Now summarize it.',
          assistantAnswer: 'Exact **final** answer.\n',
        },
      ],
    };

    const first = conversationToGraph(conversation);
    const second = conversationToGraph(conversation);

    expect(second).toEqual(first);
    expect(Array.from(first.nodes.values())).toEqual([
      {
        id: 'import:session%2Falpha:turn:0:user',
        role: 'user',
        content: 'Explain the result.',
        timestamp: 1_000,
      },
      {
        id: 'import:session%2Falpha:turn:0:assistant',
        role: 'assistant',
        content: '# Result\n\n```ts\nconst exact = true;\n```\n',
        timestamp: 2_000,
        model: 'example-model',
      },
      {
        id: 'import:session%2Falpha:turn:1:user',
        role: 'user',
        content: 'Now summarize it.',
        timestamp: 2,
      },
      {
        id: 'import:session%2Falpha:turn:1:assistant',
        role: 'assistant',
        content: 'Exact **final** answer.\n',
        timestamp: 3,
      },
    ]);
    expect(first.edges).toEqual([
      {
        id: 'import:session%2Falpha:turn:0:user->import:session%2Falpha:turn:0:assistant',
        source: 'import:session%2Falpha:turn:0:user',
        target: 'import:session%2Falpha:turn:0:assistant',
      },
      {
        id: 'import:session%2Falpha:turn:0:assistant->import:session%2Falpha:turn:1:user',
        source: 'import:session%2Falpha:turn:0:assistant',
        target: 'import:session%2Falpha:turn:1:user',
      },
      {
        id: 'import:session%2Falpha:turn:1:user->import:session%2Falpha:turn:1:assistant',
        source: 'import:session%2Falpha:turn:1:user',
        target: 'import:session%2Falpha:turn:1:assistant',
      },
    ]);
    expect(Array.from(first.layout.entries())).toEqual([
      ['import:session%2Falpha:turn:0:user', { x: 0, y: 0 }],
      ['import:session%2Falpha:turn:0:assistant', { x: 0, y: 120 }],
      ['import:session%2Falpha:turn:1:user', { x: 0, y: 240 }],
      ['import:session%2Falpha:turn:1:assistant', { x: 0, y: 360 }],
    ]);
  });

  it('selects an inclusive Turn range without renumbering included Turns', () => {
    const conversation: ImportedConversation = {
      importKey: 'range',
      turns: [
        { userMessage: 'u0', assistantAnswer: 'a0' },
        { userMessage: 'u1', assistantAnswer: 'a1' },
        { userMessage: 'u2', assistantAnswer: 'a2' },
        { userMessage: 'u3', assistantAnswer: 'a3' },
      ],
    };

    const graph = conversationToGraph(conversation, { startIndex: 1, endIndex: 2 });

    expect(Array.from(graph.nodes.keys())).toEqual([
      'import:range:turn:1:user',
      'import:range:turn:1:assistant',
      'import:range:turn:2:user',
      'import:range:turn:2:assistant',
    ]);
    expect(graph.edges).toEqual([
      {
        id: 'import:range:turn:1:user->import:range:turn:1:assistant',
        source: 'import:range:turn:1:user',
        target: 'import:range:turn:1:assistant',
      },
      {
        id: 'import:range:turn:1:assistant->import:range:turn:2:user',
        source: 'import:range:turn:1:assistant',
        target: 'import:range:turn:2:user',
      },
      {
        id: 'import:range:turn:2:user->import:range:turn:2:assistant',
        source: 'import:range:turn:2:user',
        target: 'import:range:turn:2:assistant',
      },
    ]);
    expect(Array.from(graph.layout.values())).toEqual([
      { x: 0, y: 240 },
      { x: 0, y: 360 },
      { x: 0, y: 480 },
      { x: 0, y: 600 },
    ]);
  });

  it('retains normalized Turn provenance including unknown activity', () => {
    const conversation: ImportedConversation = {
      importKey: 'provenance',
      turns: [
        {
          userMessage: 'What evidence did you use?',
          assistantAnswer: 'The answer.',
          provenance: {
            completeness: 'partial',
            references: [
              {
                type: 'url',
                url: 'https://example.com/source',
                title: 'Source',
                relations: ['consulted', 'cited'],
              },
              {
                type: 'file',
                scope: 'vault',
                path: 'notes/source.md',
                displayName: 'source.md',
                relations: ['read'],
              },
            ],
            activity: [
              { type: 'commentary', content: 'Checking evidence.' },
              {
                type: 'tool',
                kind: 'read',
                title: 'Read source.md',
                status: 'completed',
              },
              {
                type: 'unknown',
                providerType: 'provider_status',
                label: 'Unrecognized provider event',
              },
            ],
          },
        },
      ],
    };

    const graph = conversationToGraph(conversation);
    const assistant = graph.nodes.get('import:provenance:turn:0:assistant');

    expect(assistant).toMatchObject({
      role: 'assistant',
      provenance: conversation.turns[0].provenance,
    });
  });
});
