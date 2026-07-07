import type { GraphNode } from '@thoughttree/graph-model';

/** Half-open [start, end) char range into the text it accompanies. */
export interface TextSpan {
  start: number;
  end: number;
}

/** Display-ready text with matched ranges. Spans are sorted and non-overlapping. */
export interface HighlightedText {
  text: string;
  spans: TextSpan[];
}

export interface NodeHit {
  kind: 'node';
  /** Ref into the corpus snapshot; safe because GraphModel mutations are pure. */
  node: GraphNode;
  /** Node summary, else content head. */
  title: HighlightedText;
  /** Content line around the first match; absent when the match is summary-only. */
  snippet?: HighlightedText;
}

export type SearchHit = NodeHit;
