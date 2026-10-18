export type Side = 'top' | 'bottom' | 'left' | 'right';

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface EdgeAnchors {
  sourceX: number;
  sourceY: number;
  sourceSide: Side;
  targetX: number;
  targetY: number;
  targetSide: Side;
}

function midpoint(r: Rect, side: Side) {
  switch (side) {
    case 'top':
      return { x: r.x + r.width / 2, y: r.y };
    case 'bottom':
      return { x: r.x + r.width / 2, y: r.y + r.height };
    case 'left':
      return { x: r.x, y: r.y + r.height / 2 };
    case 'right':
      return { x: r.x + r.width, y: r.y + r.height / 2 };
  }
}

/**
 * Where an edge should leave and enter two nodes, for layouts that grow in any
 * direction. The edge uses the facing sides' midpoints, chosen by whichever
 * axis separates the centres more (relative to node size). A child below its
 * parent gets bottom -> top, exactly like the fixed handles.
 */
export function edgeAnchors(source: Rect, target: Rect): EdgeAnchors {
  const dx = target.x + target.width / 2 - (source.x + source.width / 2);
  const dy = target.y + target.height / 2 - (source.y + source.height / 2);
  const vertical = Math.abs(dy) / (source.height + target.height) >= Math.abs(dx) / (source.width + target.width);

  let sourceSide: Side;
  let targetSide: Side;
  if (vertical) {
    sourceSide = dy >= 0 ? 'bottom' : 'top';
    targetSide = dy >= 0 ? 'top' : 'bottom';
  } else {
    sourceSide = dx >= 0 ? 'right' : 'left';
    targetSide = dx >= 0 ? 'left' : 'right';
  }

  const s = midpoint(source, sourceSide);
  const t = midpoint(target, targetSide);
  return { sourceX: s.x, sourceY: s.y, sourceSide, targetX: t.x, targetY: t.y, targetSide };
}
