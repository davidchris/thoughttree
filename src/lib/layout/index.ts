import { packComponents, toTopLeft, type Point } from './geometry';
import { layeredComponent } from './layered';
import { buildStructure, type LayoutEdge, type LayoutNode } from './structure';

export type { LayoutEdge, LayoutNode } from './structure';
export type { Point } from './geometry';

export interface LayoutOptions {
  gridSize?: number;
  /** Free space between neighbouring nodes. */
  gapX?: number;
  gapY?: number;
  /** Free space between separate Graph components. */
  componentGap?: number;
}

/**
 * Tidy: lay out each Graph component with Layered, then pack the components
 * into a roughly 16:10 block. Returns grid-snapped top-left positions anchored
 * at the old top-left corner. Edges with an unknown endpoint are ignored, and
 * edges that close a legacy cycle do not constrain the layers.
 */
export function computeLayout(
  nodes: LayoutNode[],
  edges: LayoutEdge[],
  options: LayoutOptions = {}
): Map<string, Point> {
  if (nodes.length === 0) return new Map();
  const gaps = { gapX: options.gapX ?? 40, gapY: options.gapY ?? 40 };
  const s = buildStructure(nodes, edges);

  const packed = packComponents(
    s.components.map((c) => layeredComponent(s, c, gaps)),
    s.nodes,
    options.componentGap ?? 120
  );
  const anchor = {
    x: Math.min(...nodes.map((n) => n.x)),
    y: Math.min(...nodes.map((n) => n.y)),
  };
  return toTopLeft(packed, s.nodes, anchor, options.gridSize ?? 20);
}
