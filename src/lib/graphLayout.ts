import type { Edge, Node } from '@xyflow/react';

export type LayoutDirection = 'TB' | 'LR';

export interface AutoLayoutOptions {
  direction?: LayoutDirection;
  gridSize?: number;
  nodeGap?: number;
  levelGap?: number;
}

type Pos = { x: number; y: number };

type NodeIndex = Map<string, Node>;
/** child id -> the one parent we lay it out under. */
type ParentMap = Map<string, string>;
/** parent id -> child ids, ordered left to right. */
type ChildMap = Map<string, string[]>;

function snapToGrid(value: number, gridSize: number) {
  if (!gridSize) return value;
  return Math.round(value / gridSize) * gridSize;
}

/**
 * Pick one parent per node: the first incoming edge wins, so the graph is
 * reduced to a tree. Edges touching unknown nodes are ignored.
 */
function buildParentMap(nodeById: NodeIndex, edges: Array<Edge>): ParentMap {
  const parent: ParentMap = new Map();
  for (const e of edges) {
    if (!e.source || !e.target) continue;
    if (!nodeById.has(e.source) || !nodeById.has(e.target)) continue;
    if (!parent.has(e.target)) parent.set(e.target, e.source);
  }
  return parent;
}

/**
 * Nodes with no incoming edge, topmost first. A graph made only of cycles has
 * no such node, so every node is treated as a root instead.
 */
function pickRoots(nodes: Array<Node>, parent: ParentMap): Array<Node> {
  const roots = nodes.filter((n) => !parent.has(n.id));
  return (roots.length ? roots : nodes).slice().sort((a, b) => {
    if (a.position.y !== b.position.y) return a.position.y - b.position.y;
    return a.position.x - b.position.x;
  });
}

/** Invert the parent map, ordering siblings by current x so layout is stable. */
function buildChildMap(nodeById: NodeIndex, parent: ParentMap): ChildMap {
  const children: ChildMap = new Map();
  for (const [childId, parentId] of parent) {
    children.set(parentId, [...(children.get(parentId) ?? []), childId]);
  }

  for (const kids of children.values()) {
    kids.sort((a, b) => {
      const na = nodeById.get(a);
      const nb = nodeById.get(b);
      if (!na || !nb) return 0;
      if (na.position.x !== nb.position.x) return na.position.x - nb.position.x;
      return na.position.y - nb.position.y;
    });
  }
  return children;
}

/**
 * Distance from the root, following the parent chain. Nodes sitting on a parent
 * cycle get no depth at all and are left unplaced.
 */
function computeDepths(nodes: Array<Node>, parent: ParentMap): Map<string, number> {
  const depth = new Map<string, number>();
  const visiting = new Set<string>();

  const depthOf = (id: string): number | null => {
    const known = depth.get(id);
    if (known !== undefined) return known;
    if (visiting.has(id)) return null; // cycle

    visiting.add(id);
    const parentId = parent.get(id);
    let d = 0;
    if (parentId !== undefined) {
      const parentDepth = depthOf(parentId);
      if (parentDepth === null) {
        visiting.delete(id);
        return null;
      }
      d = parentDepth + 1;
    }
    visiting.delete(id);

    depth.set(id, d);
    return d;
  };

  for (const n of nodes) depthOf(n.id);
  return depth;
}

/**
 * Width of each subtree, measured in column slots:
 * - no children -> 1 slot
 * - one child -> the child's width (they share a column)
 * - several children -> the sum of their widths
 */
function computeSubtreeWidths(roots: Array<Node>, children: ChildMap): Map<string, number> {
  const width = new Map<string, number>();

  const widthOf = (id: string): number => {
    const known = width.get(id);
    if (known !== undefined) return known;

    const kids = children.get(id) ?? [];
    let w = 1;
    if (kids.length === 1) w = widthOf(kids[0]);
    else if (kids.length > 1) w = kids.reduce((sum, k) => sum + widthOf(k), 0);

    width.set(id, w);
    return w;
  };

  for (const r of roots) widthOf(r.id);
  return width;
}

/**
 * Column slot per node. A single child stays in its parent's column; several
 * children are spread out and the parent is centred above them. Separate trees
 * are kept one empty slot apart.
 */
function assignSlots(
  roots: Array<Node>,
  children: ChildMap,
  subtreeWidth: Map<string, number>
): Map<string, number> {
  const slot = new Map<string, number>();

  const place = (id: string, startSlot: number): void => {
    const kids = children.get(id) ?? [];

    if (kids.length === 0) {
      slot.set(id, startSlot);
      return;
    }

    if (kids.length === 1) {
      place(kids[0], startSlot);
      slot.set(id, slot.get(kids[0])!);
      return;
    }

    let cursor = startSlot;
    for (const k of kids) {
      place(k, cursor);
      cursor += subtreeWidth.get(k) ?? 1;
    }

    const firstKidSlot = slot.get(kids[0])!;
    const lastKidSlot = slot.get(kids[kids.length - 1])!;
    slot.set(id, (firstKidSlot + lastKidSlot) / 2);
  };

  let nextFreeSlot = 0;
  for (const r of roots) {
    place(r.id, nextFreeSlot);
    nextFreeSlot += (subtreeWidth.get(r.id) ?? 1) + 1;
  }
  return slot;
}

/** Current top-left corner of the selection, used to anchor the new layout. */
function currentTopLeft(nodes: Array<Node>): Pos {
  let x = Infinity;
  let y = Infinity;
  for (const n of nodes) {
    x = Math.min(x, n.position.x);
    y = Math.min(y, n.position.y);
  }
  return {
    x: Number.isFinite(x) ? x : 0,
    y: Number.isFinite(y) ? y : 0,
  };
}

/** Smallest slot and depth among the nodes that actually got placed. */
function layoutOrigin(
  nodes: Array<Node>,
  slot: Map<string, number>,
  depth: Map<string, number>
): { minSlot: number; minDepth: number } {
  let minSlot = Infinity;
  let minDepth = Infinity;
  for (const n of nodes) {
    const s = slot.get(n.id);
    const d = depth.get(n.id);
    if (s === undefined || d === undefined) continue;
    minSlot = Math.min(minSlot, s);
    minDepth = Math.min(minDepth, d);
  }
  return {
    minSlot: Number.isFinite(minSlot) ? minSlot : 0,
    minDepth: Number.isFinite(minDepth) ? minDepth : 0,
  };
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

  // Slot pitch, centre to centre. Nodes are 170x120, so both leave a 40px gap.
  const nodeGap = options.nodeGap ?? 210; // horizontal pitch between siblings
  const levelGap = options.levelGap ?? 160; // vertical pitch between levels

  if (nodes.length === 0) return new Map();

  const nodeById: NodeIndex = new Map(nodes.map((n) => [n.id, n] as const));
  const parent = buildParentMap(nodeById, edges);
  const rootNodes = pickRoots(nodes, parent);
  const children = buildChildMap(nodeById, parent);

  const depth = computeDepths(nodes, parent);
  const subtreeWidth = computeSubtreeWidths(rootNodes, children);
  const slot = assignSlots(rootNodes, children, subtreeWidth);

  const anchor = currentTopLeft(nodes);
  const { minSlot, minDepth } = layoutOrigin(nodes, slot, depth);

  // Convert slots to pixel positions
  const pos = new Map<string, Pos>();
  for (const n of nodes) {
    const nodeSlot = slot.get(n.id);
    const nodeDepth = depth.get(n.id);

    if (nodeSlot === undefined || nodeDepth === undefined) {
      // Fallback: keep position but snap to grid
      pos.set(n.id, {
        x: snapToGrid(n.position.x, gridSize),
        y: snapToGrid(n.position.y, gridSize),
      });
      continue;
    }

    const acrossLevel = (nodeSlot - minSlot) * nodeGap;
    const alongLevels = (nodeDepth - minDepth) * levelGap;

    const layout =
      direction === 'TB'
        ? { x: anchor.x + acrossLevel, y: anchor.y + alongLevels }
        : { x: anchor.x + alongLevels, y: anchor.y + acrossLevel };

    pos.set(n.id, {
      x: snapToGrid(layout.x, gridSize),
      y: snapToGrid(layout.y, gridSize),
    });
  }

  return pos;
}
