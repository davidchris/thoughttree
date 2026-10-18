import { packComponents, toTopLeft, type Placement, type Point } from './geometry';
import { foldedComponent, layeredComponent } from './layered';
import { organicComponent } from './organic';
import { radialComponent } from './radial';
import { buildStructure, type LayoutEdge, type LayoutNode, type Structure } from './structure';

export type { LayoutEdge, LayoutNode } from './structure';
export type { Point } from './geometry';

/** Every tidy style. `tree` is the original one-row tidy tree in graphLayout.ts. */
export type LayoutStyle = 'tree' | 'layered' | 'folded' | 'radial' | 'organic';

export const LAYOUT_STYLES: ReadonlyArray<{ id: LayoutStyle; label: string; hint: string }> = [
  { id: 'tree', label: 'Tree', hint: 'Original: one row of top-down trees' },
  { id: 'layered', label: 'Layered', hint: 'Top-down DAG; merges sit between their parents' },
  { id: 'folded', label: 'Folded', hint: 'Layered, long threads snake down and back up' },
  { id: 'radial', label: 'Radial', hint: 'Rings around the middle of each conversation' },
  { id: 'organic', label: 'Organic', hint: 'Force-directed; threads fold into free space' },
];

export interface StyledLayoutOptions {
  gridSize?: number;
  /** Free space between neighbouring nodes. */
  gapX?: number;
  gapY?: number;
  /** Free space between separate conversations. */
  componentGap?: number;
}

type ComponentLayout = (s: Structure, component: string[], gaps: { gapX: number; gapY: number }) => Placement;

const COMPONENT_LAYOUTS: Record<Exclude<LayoutStyle, 'tree'>, ComponentLayout> = {
  layered: layeredComponent,
  folded: foldedComponent,
  radial: (s, c, { gapX }) => radialComponent(s, c, { ringGap: 190, gap: gapX }),
  organic: organicComponent,
};

/**
 * Lay out each connected conversation (trees joined by a merge count as one)
 * with the chosen style, then pack them into a roughly 16:10 block. Returns
 * top-left positions anchored at the old top-left corner.
 */
export function computeStyledLayout(
  style: Exclude<LayoutStyle, 'tree'>,
  nodes: LayoutNode[],
  edges: LayoutEdge[],
  options: StyledLayoutOptions = {}
): Map<string, Point> {
  if (nodes.length === 0) return new Map();
  const gaps = { gapX: options.gapX ?? 40, gapY: options.gapY ?? 40 };
  const s = buildStructure(nodes, edges);
  const layout = COMPONENT_LAYOUTS[style];

  const packed = packComponents(
    s.components.map((c) => layout(s, c, gaps)),
    s.nodes,
    options.componentGap ?? 120
  );
  const anchor = {
    x: Math.min(...nodes.map((n) => n.x)),
    y: Math.min(...nodes.map((n) => n.y)),
  };
  return toTopLeft(packed, s.nodes, anchor, options.gridSize ?? 20);
}
