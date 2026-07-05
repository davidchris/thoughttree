import { describe, it, expect } from 'vitest';
import { PaletteSearch } from './search';
import type { GraphNode } from '../graph';
import type { UserNodeData, AgentNodeData } from '../../types';

function userNode(id: string, overrides: Partial<UserNodeData> = {}): GraphNode {
  return { id, role: 'user', content: '', timestamp: 0, ...overrides };
}

function agentNode(id: string, overrides: Partial<AgentNodeData> = {}): GraphNode {
  return { id, role: 'assistant', content: '', timestamp: 0, ...overrides };
}

describe('PaletteSearch.search', () => {
  it('finds a node by case-insensitive substring of its content', () => {
    const corpus = [userNode('a', { content: 'The Borrow checker fights the visitor' })];

    const { hits } = PaletteSearch.search(corpus, 'borrow');

    expect(hits).toHaveLength(1);
    expect(hits[0].kind).toBe('node');
    expect(hits[0].node.id).toBe('a');
  });

  it('requires every whitespace-separated token to match (AND semantics)', () => {
    const corpus = [
      userNode('both', { content: 'the borrow checker fights the visitor pattern' }),
      userNode('only-one', { content: 'the borrow checker is strict' }),
    ];

    const { hits } = PaletteSearch.search(corpus, 'visitor borrow');

    expect(hits.map((h) => h.node.id)).toEqual(['both']);
  });

  it('matches against the summary too, so images-only nodes stay findable', () => {
    const corpus = [
      userNode('images-only', { content: '', summary: 'Parser architecture diagram' }),
    ];

    const { hits } = PaletteSearch.search(corpus, 'parser');

    expect(hits.map((h) => h.node.id)).toEqual(['images-only']);
  });

  it('treats regex metacharacters in the query as literal text', () => {
    const corpus = [
      userNode('a', { content: 'the a+b expression' }),
      userNode('b', { content: 'aaab does not contain the literal' }),
    ];

    const { hits } = PaletteSearch.search(corpus, 'a+b');

    expect(hits.map((h) => h.node.id)).toEqual(['a']);
  });

  it('ranks summary matches above content-only matches', () => {
    const corpus = [
      agentNode('in-content', { content: 'a long answer mentioning parsers midway' }),
      agentNode('in-summary', { content: 'unrelated body', summary: 'Parser design' }),
    ];

    const { hits } = PaletteSearch.search(corpus, 'parser');

    expect(hits.map((h) => h.node.id)).toEqual(['in-summary', 'in-content']);
  });

  it('breaks ties by earliest match position in content', () => {
    const corpus = [
      agentNode('late', { content: 'a very long preamble before the word parser appears' }),
      agentNode('early', { content: 'parser talk starts immediately' }),
    ];

    const { hits } = PaletteSearch.search(corpus, 'parser');

    expect(hits.map((h) => h.node.id)).toEqual(['early', 'late']);
  });

  it('breaks remaining ties by recency, falling back to timestamp when contentUpdatedAt is absent', () => {
    const corpus = [
      userNode('old', { content: 'parser question', timestamp: 100 }),
      userNode('new', { content: 'parser question', timestamp: 100, contentUpdatedAt: 900 }),
      userNode('mid', { content: 'parser question', timestamp: 500 }),
    ];

    const { hits } = PaletteSearch.search(corpus, 'parser');

    expect(hits.map((h) => h.node.id)).toEqual(['new', 'mid', 'old']);
  });

  it('returns recently updated nodes for an empty query, excluding empty nodes', () => {
    const corpus = [
      userNode('oldest', { content: 'first thought', timestamp: 100 }),
      agentNode('newest', { content: 'latest answer', timestamp: 200, contentUpdatedAt: 900 }),
      userNode('empty', { content: '', timestamp: 950 }),
      userNode('middle', { content: 'follow-up', timestamp: 500 }),
    ];

    const { hits } = PaletteSearch.search(corpus, '');

    expect(hits.map((h) => h.node.id)).toEqual(['newest', 'middle', 'oldest']);
  });

  it('caps materialized hits at the limit while reporting the total match count', () => {
    const corpus = [
      userNode('a', { content: 'parser one', timestamp: 3 }),
      userNode('b', { content: 'parser two', timestamp: 2 }),
      userNode('c', { content: 'parser three', timestamp: 1 }),
    ];

    const { hits, total } = PaletteSearch.search(corpus, 'parser', 2);

    expect(hits).toHaveLength(2);
    expect(total).toBe(3);
  });

  it('uses the summary as title, falling back to the first content line', () => {
    const corpus = [
      agentNode('summarized', { content: 'long body about parsers', summary: 'Parser overview' }),
      agentNode('raw', { content: 'first line about parsers\nsecond line' }),
    ];

    const { hits } = PaletteSearch.search(corpus, 'parsers');

    const byId = new Map(hits.map((h) => [h.node.id, h]));
    expect(byId.get('summarized')?.title.text).toBe('Parser overview');
    expect(byId.get('raw')?.title.text).toBe('first line about parsers');
  });

  it('skips leading blank lines when deriving the title from content', () => {
    const corpus = [userNode('a', { content: '\n\nThe borrow checker fights', timestamp: 1 })];

    const { hits } = PaletteSearch.search(corpus, '');

    expect(hits[0].title.text).toBe('The borrow checker fights');
  });

  it('never cuts a surrogate pair at the title truncation boundary', () => {
    const corpus = [userNode('a', { content: 'x'.repeat(79) + '😀 and more', timestamp: 1 })];

    const { hits } = PaletteSearch.search(corpus, '');

    expect(hits[0].title.text).toBe('x'.repeat(79));
    expect(/[\ud800-\udbff]$/.test(hits[0].title.text)).toBe(false);
  });

  it('marks matched tokens in the title with sorted character spans', () => {
    const corpus = [agentNode('a', { content: 'body', summary: 'Parser design notes' })];

    const { hits } = PaletteSearch.search(corpus, 'design parser');

    expect(hits[0].title.spans).toEqual([
      { start: 0, end: 6 }, // "Parser"
      { start: 7, end: 13 }, // "design"
    ]);
  });

  it('provides a bounded content snippet around the first match, spans relative to the snippet', () => {
    const longPrefix = 'irrelevant preamble '.repeat(20); // match sits deep in the content
    const corpus = [
      agentNode('a', { content: `${longPrefix}the Parser lives here and more text follows` }),
    ];

    const { hits } = PaletteSearch.search(corpus, 'parser');

    const snippet = hits[0].snippet;
    expect(snippet).toBeDefined();
    expect(snippet!.text.length).toBeLessThanOrEqual(100);
    expect(snippet!.spans.length).toBeGreaterThan(0);
    const [span] = snippet!.spans;
    expect(snippet!.text.slice(span.start, span.end)).toBe('Parser');
  });

  it('keeps the snippet centered on the match when earlier characters change length under lowercasing', () => {
    // 'İ'.toLowerCase() is two code units; positions must come from the
    // original string, not a lowercased copy, or the window drifts.
    const corpus = [
      agentNode('a', {
        content: 'İ'.repeat(40) + ' needle sits here ' + 'tail words '.repeat(20),
      }),
    ];

    const { hits } = PaletteSearch.search(corpus, 'needle');

    const snippet = hits[0].snippet;
    expect(snippet).toBeDefined();
    expect(snippet!.spans.length).toBeGreaterThan(0);
    const [span] = snippet!.spans;
    expect(snippet!.text.slice(span.start, span.end)).toBe('needle');
  });

  it('omits the snippet when the match is summary-only', () => {
    const corpus = [agentNode('a', { content: 'nothing relevant', summary: 'Parser design' })];

    const { hits } = PaletteSearch.search(corpus, 'parser');

    expect(hits[0].snippet).toBeUndefined();
  });

  it('omits the snippet for empty queries (recent list)', () => {
    const corpus = [agentNode('a', { content: 'some content', timestamp: 1 })];

    const { hits } = PaletteSearch.search(corpus, '');

    expect(hits[0].snippet).toBeUndefined();
  });
});
