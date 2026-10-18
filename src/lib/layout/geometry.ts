import type { LayoutNode } from './structure';

export type Point = { x: number; y: number };

/** Node centres. Layouts think in centres; the canvas wants top-left corners. */
export type Placement = Map<string, Point>;

export interface Bounds {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

export function bounds(placement: Placement, nodes: Map<string, LayoutNode>): Bounds {
  const b = { minX: Infinity, minY: Infinity, maxX: -Infinity, maxY: -Infinity };
  for (const [id, p] of placement) {
    const n = nodes.get(id);
    const hw = (n?.width ?? 0) / 2;
    const hh = (n?.height ?? 0) / 2;
    b.minX = Math.min(b.minX, p.x - hw);
    b.minY = Math.min(b.minY, p.y - hh);
    b.maxX = Math.max(b.maxX, p.x + hw);
    b.maxY = Math.max(b.maxY, p.y + hh);
  }
  return b;
}

/**
 * Push overlapping boxes apart along the axis that needs the smaller move.
 * Boxes are sorted by x so each pass only compares horizontal neighbours.
 */
export function resolveOverlaps(
  placement: Placement,
  nodes: Map<string, LayoutNode>,
  gap: number,
  maxPasses = 100
): void {
  const ids = [...placement.keys()].filter((id) => nodes.has(id));
  const maxWidth = Math.max(0, ...ids.map((id) => nodes.get(id)!.width));

  // Push box i away from every box to its right that could still touch it.
  const separateFromRight = (i: number) => {
    const a = placement.get(ids[i])!;
    let moved = false;
    for (let j = i + 1; j < ids.length; j++) {
      const b = placement.get(ids[j])!;
      if (b.x - a.x >= maxWidth + gap) break;
      if (separate(a, nodes.get(ids[i])!, b, nodes.get(ids[j])!, gap, i + j)) moved = true;
    }
    return moved;
  };

  for (let pass = 0; pass < maxPasses; pass++) {
    ids.sort((a, b) => placement.get(a)!.x - placement.get(b)!.x);
    let moved = false;
    for (let i = 0; i < ids.length; i++) moved = separateFromRight(i) || moved;
    if (!moved) return;
  }
}

/** Move two overlapping boxes half the overlap each. Returns true when they moved. */
function separate(a: Point, na: LayoutNode, b: Point, nb: LayoutNode, gap: number, salt: number) {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const overlapX = (na.width + nb.width) / 2 + gap - Math.abs(dx);
  const overlapY = (na.height + nb.height) / 2 + gap - Math.abs(dy);
  if (overlapX <= 0.5 || overlapY <= 0.5) return false;

  // Coincident centres have no direction; pick one deterministically.
  const sign = (d: number) => (d === 0 ? (salt % 2 ? 1 : -1) : Math.sign(d));
  if (overlapX < overlapY) {
    const push = (overlapX / 2) * sign(dx);
    a.x -= push;
    b.x += push;
  } else {
    const push = (overlapY / 2) * sign(dy);
    a.y -= push;
    b.y += push;
  }
  return true;
}

/**
 * Arrange independent components in rows ("shelves") so the whole canvas comes
 * out roughly `aspect` wide per unit tall instead of one long horizontal strip.
 * Bigger components go first; each keeps its own internal layout.
 */
export function packComponents(
  layouts: Placement[],
  nodes: Map<string, LayoutNode>,
  gap: number,
  aspect = 1.6
): Placement {
  const boxes = layouts.map((placement, index) => {
    const b = bounds(placement, nodes);
    return { placement, index, b, w: b.maxX - b.minX, h: b.maxY - b.minY };
  });
  const area = boxes.reduce((sum, box) => sum + (box.w + gap) * (box.h + gap), 0);
  const widest = Math.max(0, ...boxes.map((box) => box.w));
  const rowWidth = Math.max(widest, Math.sqrt(area * aspect));

  // Tallest first packs shelves tightly; ties keep reading order.
  boxes.sort((a, b) => b.h - a.h || b.w - a.w || a.index - b.index);

  const out: Placement = new Map();
  let cursorX = 0;
  let cursorY = 0;
  let shelfHeight = 0;
  for (const box of boxes) {
    if (cursorX > 0 && cursorX + box.w > rowWidth) {
      cursorX = 0;
      cursorY += shelfHeight + gap;
      shelfHeight = 0;
    }
    for (const [id, p] of box.placement) {
      out.set(id, { x: p.x - box.b.minX + cursorX, y: p.y - box.b.minY + cursorY });
    }
    cursorX += box.w + gap;
    shelfHeight = Math.max(shelfHeight, box.h);
  }
  return out;
}

export function snap(value: number, grid: number) {
  return grid ? Math.round(value / grid) * grid : value;
}

/**
 * Convert centres to top-left corners, anchored so the new layout starts at
 * `anchor` (the old top-left), and snapped to the grid.
 */
export function toTopLeft(
  placement: Placement,
  nodes: Map<string, LayoutNode>,
  anchor: Point,
  grid: number
): Map<string, Point> {
  const b = bounds(placement, nodes);
  const out = new Map<string, Point>();
  for (const [id, p] of placement) {
    const n = nodes.get(id);
    if (!n) continue;
    out.set(id, {
      x: snap(anchor.x + p.x - b.minX - n.width / 2, grid),
      y: snap(anchor.y + p.y - b.minY - n.height / 2, grid),
    });
  }
  return out;
}
