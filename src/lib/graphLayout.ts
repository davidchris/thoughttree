import type { Edge, Node } from '@xyflow/react';

export type LayoutDirection = 'TB' | 'LR';

export interface AutoLayoutOptions {
  direction?: LayoutDirection;
  gridSize?: number;
  nodeGap?: number;
  levelGap?: number;
}

type Pos = { x: number; y: number };

function snapToGrid(value: number, gridSize: number) {
  if (!gridSize) return value;
  return Math.round(value / gridSize) * gridSize;
}

/**
 * Tidy tree layout for ThoughtTree graphs.
 *
 * Rules:
 * - Single child: placed directly below parent (no horizontal offset)
 * - Multiple children: spread horizontally, parent centered above
 * - Tight spacing for a clean, compact look
 */
export function computeAutoLayout(
  nodes: Array<Node>,
  edges: Array<Edge>,
  options: AutoLayoutOptions = {}
): Map<string, Pos> {
  const direction: LayoutDirection = options.direction ?? 'TB';
  const gridSize = options.gridSize ?? 20;

  // Tighter spacing for a cleaner look
  const nodeGap = options.nodeGap ?? 160; // horizontal gap between siblings
  const levelGap = options.levelGap ?? 160; // vertical gap between levels

  if (nodes.length === 0) return new Map();

  const nodeById = new Map(nodes.map((n) => [n.id, n] as const));

  // Build edge maps
  const incoming = new Map<string, string[]>();
  const outgoing = new Map<string, string[]>();

  for (const e of edges) {
    if (!e.source || !e.target) continue;
    if (!nodeById.has(e.source) || !nodeById.has(e.target)) continue;
    outgoing.set(e.source, [...(outgoing.get(e.source) ?? []), e.target]);
    incoming.set(e.target, [...(incoming.get(e.target) ?? []), e.source]);
  }

  // Roots: nodes with no incoming edges
  const roots = nodes.filter((n) => (incoming.get(n.id)?.length ?? 0) === 0);
  const rootNodes = (roots.length ? roots : nodes).slice().sort((a, b) => {
    if (a.position.y !== b.position.y) return a.position.y - b.position.y;
    return a.position.x - b.position.x;
  });

  // Choose a single parent for each node (first incoming edge)
  const parent = new Map<string, string | undefined>();
  for (const n of nodes) {
    const parents = incoming.get(n.id);
    if (parents && parents.length) parent.set(n.id, parents[0]);
  }

  // Build children map from parent relationships
  const children = new Map<string, string[]>();
  for (const [childId, parentId] of parent.entries()) {
    if (!parentId) continue;
    children.set(parentId, [...(children.get(parentId) ?? []), childId]);
  }

  // Sort children by current x position for stable ordering
  for (const [pid, kids] of children.entries()) {
    kids.sort((a, b) => {
      const na = nodeById.get(a);
      const nb = nodeById.get(b);
      if (!na || !nb) return 0;
      if (na.position.x !== nb.position.x) return na.position.x - nb.position.x;
      return na.position.y - nb.position.y;
    });
    children.set(pid, kids);
  }

  // Compute depth via parent chain
  const depth = new Map<string, number>();
  const visiting = new Set<string>();

  const computeDepth = (id: string): number | null => {
    if (depth.has(id)) return depth.get(id)!;
    if (visiting.has(id)) return null; // cycle
    visiting.add(id);
    const p = parent.get(id);
    let d = 0;
    if (p) {
      const pd = computeDepth(p);
      if (pd === null) {
        visiting.delete(id);
        return null;
      }
      d = pd + 1;
    }
    visiting.delete(id);
    depth.set(id, d);
    return d;
  };

  for (const n of nodes) computeDepth(n.id);

  // Compute subtree width for each node (in "slots")
  // A node with no children = 1 slot
  // A node with 1 child = same width as child (stacks vertically)
  // A node with N children = sum of children widths
  const subtreeWidth = new Map<string, number>();
  const computeWidth = (id: string): number => {
    if (subtreeWidth.has(id)) return subtreeWidth.get(id)!;

    const kids = children.get(id) ?? [];
    if (kids.length === 0) {
      subtreeWidth.set(id, 1);
      return 1;
    }
    if (kids.length === 1) {
      // Single child: same column, no extra width
      const w = computeWidth(kids[0]);
      subtreeWidth.set(id, w);
      return w;
    }
    // Multiple children: sum their widths
    const w = kids.reduce((sum, k) => sum + computeWidth(k), 0);
    subtreeWidth.set(id, w);
    return w;
  };

  for (const r of rootNodes) computeWidth(r.id);

  // Assign x positions (slot-based)
  const xSlot = new Map<string, number>();

  const assignX = (id: string, startSlot: number): void => {
    const kids = children.get(id) ?? [];

    if (kids.length === 0) {
      // Leaf: place at startSlot
      xSlot.set(id, startSlot);
      return;
    }

    if (kids.length === 1) {
      // Single child: parent and child share the same x
      assignX(kids[0], startSlot);
      const childSlot = xSlot.get(kids[0])!;
      xSlot.set(id, childSlot);
      return;
    }

    // Multiple children: spread them out, center parent
    let currentSlot = startSlot;
    for (const k of kids) {
      assignX(k, currentSlot);
      currentSlot += subtreeWidth.get(k) ?? 1;
    }

    // Parent centered above children
    const firstKidSlot = xSlot.get(kids[0])!;
    const lastKidSlot = xSlot.get(kids[kids.length - 1])!;
    const centerSlot = (firstKidSlot + lastKidSlot) / 2;
    xSlot.set(id, centerSlot);
  };

  let globalSlot = 0;
  for (const r of rootNodes) {
    assignX(r.id, globalSlot);
    globalSlot += (subtreeWidth.get(r.id) ?? 1) + 1; // gap between trees
  }

  // Anchor layout near current top-left
  let currentMinX = Infinity;
  let currentMinY = Infinity;
  for (const n of nodes) {
    currentMinX = Math.min(currentMinX, n.position.x);
    currentMinY = Math.min(currentMinY, n.position.y);
  }
  if (!Number.isFinite(currentMinX)) currentMinX = 0;
  if (!Number.isFinite(currentMinY)) currentMinY = 0;

  // Find min slot/depth for offset calculation
  const layoutIds = nodes
    .map((n) => n.id)
    .filter((id) => typeof xSlot.get(id) === 'number' && typeof depth.get(id) === 'number');

  const minSlot = layoutIds.reduce((m, id) => Math.min(m, xSlot.get(id)!), Infinity);
  const minDepth = layoutIds.reduce((m, id) => Math.min(m, depth.get(id)!), Infinity);

  // Convert slots to pixel positions
  const pos = new Map<string, Pos>();
  for (const n of nodes) {
    const slot = xSlot.get(n.id);
    const d = depth.get(n.id);

    if (typeof slot !== 'number' || typeof d !== 'number') {
      // Fallback: keep position but snap to grid
      pos.set(n.id, {
        x: snapToGrid(n.position.x, gridSize),
        y: snapToGrid(n.position.y, gridSize),
      });
      continue;
    }

    const localX = (slot - (Number.isFinite(minSlot) ? minSlot : 0)) * nodeGap;
    const localY = (d - (Number.isFinite(minDepth) ? minDepth : 0)) * levelGap;

    const layoutX = direction === 'TB' ? currentMinX + localX : currentMinX + localY;
    const layoutY = direction === 'TB' ? currentMinY + localY : currentMinY + localX;

    pos.set(n.id, {
      x: snapToGrid(layoutX, gridSize),
      y: snapToGrid(layoutY, gridSize),
    });
  }

  return pos;
}
