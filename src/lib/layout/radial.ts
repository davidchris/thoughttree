import { resolveOverlaps, type Placement } from './geometry';
import { leafCounts, type SpanningTree, type Structure } from './structure';

/**
 * Radial layout around the middle of the conversation. Edge direction is
 * ignored when choosing the middle: the centre is the midpoint of the longest
 * path, which halves the radius compared with putting the root there, and
 * puts a merge between two trees in the middle with both trees around it.
 * Every node gets an angular wedge proportional to its leaves; deeper nodes
 * sit on wider rings, so threads grow outwards in all directions.
 */

export interface RadialOptions {
  ringGap: number;
  gap: number;
}

function neighbours(s: Structure): (id: string) => string[] {
  return (id) => [...(s.children.get(id) ?? []), ...(s.parents.get(id) ?? [])];
}

/** Breadth-first tree over undirected edges, keeping the visit order. */
function bfsTree(next: (id: string) => string[], root: string): SpanningTree & { order: string[] } {
  const parent = new Map<string, string>();
  const children = new Map<string, string[]>();
  const order = [root];
  const seen = new Set(order);
  for (let i = 0; i < order.length; i++) {
    for (const n of next(order[i])) {
      if (seen.has(n)) continue;
      seen.add(n);
      parent.set(n, order[i]);
      children.set(order[i], [...(children.get(order[i]) ?? []), n]);
      order.push(n);
    }
  }
  return { root, parent, children, order };
}

/** Midpoint of the longest path (double sweep; exact on trees, close on DAGs). */
function centre(next: (id: string) => string[], start: string): string {
  const first = bfsTree(next, start);
  const far = bfsTree(next, first.order[first.order.length - 1]);
  const path = [far.order[far.order.length - 1]];
  while (far.parent.has(path[path.length - 1])) path.push(far.parent.get(path[path.length - 1])!);
  return path[Math.floor(path.length / 2)];
}

export function radialComponent(
  s: Structure,
  component: string[],
  { ringGap, gap }: RadialOptions
): Placement {
  const next = neighbours(s);
  const tree = bfsTree(next, centre(next, component[0]));
  const leaves = leafCounts(tree);

  // Breadth-first: wedges and per-ring counts.
  const depth = new Map<string, number>([[tree.root, 0]]);
  const angle = new Map<string, number>([[tree.root, 0]]);
  const ringCount: number[] = [1];
  const queue: Array<{ id: string; from: number; to: number }> = [
    { id: tree.root, from: 0, to: 2 * Math.PI },
  ];
  for (let i = 0; i < queue.length; i++) {
    const { id, from, to } = queue[i];
    const kids = tree.children.get(id) ?? [];
    const total = kids.reduce((sum, k) => sum + leaves.get(k)!, 0);
    let cursor = from;
    for (const k of kids) {
      const share = ((to - from) * leaves.get(k)!) / total;
      const d = depth.get(id)! + 1;
      depth.set(k, d);
      angle.set(k, cursor + share / 2);
      ringCount[d] = (ringCount[d] ?? 0) + 1;
      queue.push({ id: k, from: cursor, to: cursor + share });
      cursor += share;
    }
  }

  // A ring is at least one step out, and big enough to seat everyone on it.
  const seat = Math.max(...component.map((id) => s.nodes.get(id)!.width)) + gap;
  const radius = [0];
  for (let d = 1; d < ringCount.length; d++) {
    radius[d] = Math.max(radius[d - 1] + ringGap, (ringCount[d] * seat) / (2 * Math.PI));
  }

  const placement: Placement = new Map();
  for (const [id, d] of depth) {
    if (!s.nodes.has(id)) continue;
    const a = angle.get(id)!;
    // Angle 0 points up; angles grow clockwise.
    placement.set(id, { x: radius[d] * Math.sin(a), y: -radius[d] * Math.cos(a) });
  }
  resolveOverlaps(placement, s.nodes, gap);
  return placement;
}
