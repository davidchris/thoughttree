import type { GraphNode } from '@thoughttree/graph-model';
import type { HighlightedText, SearchHit, TextSpan } from './types';

const TITLE_MAX_CHARS = 80;
const SNIPPET_WINDOW = 100;

export interface SearchResult {
  /** Ranked hits, capped at the caller's limit. */
  hits: SearchHit[];
  /** Total ranked matches, including those beyond the limit. */
  total: number;
}

/**
 * One compiled matcher per query token. All matching runs case-insensitively
 * against the ORIGINAL strings — never against lowercased copies — so match
 * positions are always valid indices into the displayed text (lowercasing can
 * change string length, e.g. 'İ' → 'i̇').
 */
interface TokenMatcher {
  /** First occurrence; stateless (no global flag). */
  first: RegExp;
  /** All occurrences, for span highlighting (matchAll clones it per call). */
  all: RegExp;
}

function escapeRegExp(literal: string): string {
  return literal.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function toMatchers(query: string): TokenMatcher[] {
  return query
    .split(/\s+/)
    .filter(Boolean)
    .map((token) => {
      const escaped = escapeRegExp(token);
      return { first: new RegExp(escaped, 'i'), all: new RegExp(escaped, 'gi') };
    });
}

interface Match {
  node: GraphNode;
  summaryMatched: boolean;
  /** Position of the earliest token occurrence in content; Infinity if none. */
  firstContentPos: number;
}

function matchNode(node: GraphNode, matchers: TokenMatcher[]): Match | null {
  const summary = node.summary ?? '';
  // Empty nodes have nothing to show in a result row (empty query matches everything else).
  if (node.content === '' && summary === '') return null;
  let firstContentPos = Infinity;
  let summaryMatched = false;
  for (const matcher of matchers) {
    const contentMatch = matcher.first.exec(node.content);
    const inSummary = matcher.first.test(summary);
    if (!contentMatch && !inSummary) return null;
    if (contentMatch) firstContentPos = Math.min(firstContentPos, contentMatch.index);
    if (inSummary) summaryMatched = true;
  }
  return { node, summaryMatched, firstContentPos };
}

function recency(node: GraphNode): number {
  return node.contentUpdatedAt ?? node.timestamp;
}

function compareMatches(a: Match, b: Match): number {
  if (a.summaryMatched !== b.summaryMatched) return a.summaryMatched ? -1 : 1;
  if (a.firstContentPos !== b.firstContentPos) return a.firstContentPos - b.firstContentPos;
  return recency(b.node) - recency(a.node);
}

function isLowSurrogate(code: number): boolean {
  return code >= 0xdc00 && code <= 0xdfff;
}

/** slice() that never cuts through a surrogate pair at either boundary. */
function sliceAtCodePoint(text: string, start: number, end: number): string {
  let from = start;
  let to = Math.min(end, text.length);
  if (from > 0 && from < text.length && isLowSurrogate(text.charCodeAt(from))) from += 1;
  if (to > from && to < text.length && isLowSurrogate(text.charCodeAt(to))) to -= 1;
  return text.slice(from, to);
}

function titleText(node: GraphNode): string {
  if (node.summary) return node.summary;
  const firstLine = node.content.trimStart().split('\n', 1)[0];
  return sliceAtCodePoint(firstLine, 0, TITLE_MAX_CHARS);
}

/** All occurrences of every token in text; sorted, overlaps merged. */
function highlightSpans(text: string, matchers: TokenMatcher[]): TextSpan[] {
  const raw: TextSpan[] = [];
  for (const matcher of matchers) {
    for (const found of text.matchAll(matcher.all)) {
      raw.push({ start: found.index, end: found.index + found[0].length });
    }
  }
  raw.sort((a, b) => a.start - b.start || a.end - b.end);
  const merged: TextSpan[] = [];
  for (const span of raw) {
    const last = merged[merged.length - 1];
    if (last && span.start <= last.end) {
      last.end = Math.max(last.end, span.end);
    } else {
      merged.push({ ...span });
    }
  }
  return merged;
}

/** One display line of content centered on the first match; spans relative to it. */
function extractSnippet(
  content: string,
  firstPos: number,
  matchers: TokenMatcher[]
): HighlightedText {
  const start = Math.max(0, Math.min(firstPos - 30, content.length - SNIPPET_WINDOW));
  const text = sliceAtCodePoint(content, start, start + SNIPPET_WINDOW).replace(/\s+/g, ' ');
  return { text, spans: highlightSpans(text, matchers) };
}

function toHit(match: Match, matchers: TokenMatcher[]): SearchHit {
  const title = titleText(match.node);
  const hit: SearchHit = {
    kind: 'node',
    node: match.node,
    title: { text: title, spans: highlightSpans(title, matchers) },
  };
  if (matchers.length > 0 && Number.isFinite(match.firstContentPos)) {
    hit.snippet = extractSnippet(match.node.content, match.firstContentPos, matchers);
  }
  return hit;
}

export const PaletteSearch = {
  /**
   * Search a corpus snapshot of GraphNodes.
   * query '' → recent nodes (contentUpdatedAt desc, empty nodes excluded).
   * Hits beyond `limit` are counted in `total` but not materialized —
   * span/snippet extraction only runs for the returned hits.
   */
  search(corpus: readonly GraphNode[], query: string, limit = Infinity): SearchResult {
    const matchers = toMatchers(query);
    const matches = corpus
      .map((node) => matchNode(node, matchers))
      .filter((match): match is Match => match !== null)
      .sort(compareMatches);
    return {
      hits: matches.slice(0, limit).map((match) => toHit(match, matchers)),
      total: matches.length,
    };
  },
};
