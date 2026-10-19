import type { Edge, Node } from '@xyflow/react';

import { computeLayout, type Point } from './layout';

// Keep in sync with --tt-node-width / --tt-node-height in design/tokens.css.
const DEFAULT_NODE_WIDTH = 170;
const DEFAULT_NODE_HEIGHT = 120;

export interface AutoLayoutOptions {
  gridSize?: number;
}

/** Tidy for ReactFlow nodes: Layered layout using each node's measured size. */
export function computeAutoLayout(
  nodes: Array<Node>,
  edges: Array<Edge>,
  options: AutoLayoutOptions = {}
): Map<string, Point> {
  const layoutNodes = nodes.map((n) => ({
    id: n.id,
    ...n.position,
    width: n.measured?.width ?? DEFAULT_NODE_WIDTH,
    height: n.measured?.height ?? DEFAULT_NODE_HEIGHT,
  }));
  return computeLayout(layoutNodes, edges, { gridSize: options.gridSize });
}
