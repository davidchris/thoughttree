import type { Point } from './geometry';
import type { LayoutOptions } from './index';
import type { LayoutEdge, LayoutNode } from './structure';

/**
 * One case of the layout parity fixture in `fixtures/layout/`. The TS and Rust
 * Tidy implementations must both turn `nodes` and `edges` into `expected`.
 */
export interface LayoutFixture {
  description: string;
  options: Required<LayoutOptions>;
  nodes: LayoutNode[];
  edges: LayoutEdge[];
  /** Snapped top-left position per node id. */
  expected: Record<string, Point>;
}
