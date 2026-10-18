import { resolveOverlaps, type Placement } from './geometry';
import { layeredComponent } from './layered';
import type { Structure } from './structure';

/**
 * Organic (force-directed) layout. It starts from the layered layout, so the
 * result is deterministic and untangled, then lets every edge act as a spring
 * and every pair of nodes repel (Fruchterman-Reingold). A weak pull towards the
 * middle folds long threads into the free space around them instead of
 * stretching them into one tall column.
 */

export interface OrganicOptions {
  gapX: number;
  gapY: number;
}

const GRAVITY = 0.35;

export function organicComponent(s: Structure, component: string[], opts: OrganicOptions): Placement {
  const placement = layeredComponent(s, component, opts);
  if (component.length < 3) return placement;

  const ids = component;
  const index = new Map(ids.map((id, i) => [id, i] as const));
  // A straight thread is a balanced state the forces never leave; a small,
  // repeatable sideways nudge lets it buckle and fold.
  const xs = ids.map((id, i) => placement.get(id)!.x + Math.sin(i * 2.4) * opts.gapX);
  const ys = ids.map((id) => placement.get(id)!.y);
  const springs: Array<[number, number]> = [];
  for (const id of ids) {
    for (const kid of s.children.get(id) ?? []) springs.push([index.get(id)!, index.get(kid)!]);
  }

  const maxW = Math.max(...ids.map((id) => s.nodes.get(id)!.width));
  const k = maxW + opts.gapX; // ideal edge length, centre to centre
  const iterations = ids.length > 300 ? 120 : 300;
  let temperature = k;

  for (let it = 0; it < iterations; it++) {
    const forces = { dx: new Float64Array(ids.length), dy: new Float64Array(ids.length) };
    repel(xs, ys, forces, k);
    attract(xs, ys, forces, springs, k);
    for (let i = 0; i < ids.length; i++) {
      const len = Math.hypot(forces.dx[i], forces.dy[i]);
      if (len === 0) continue;
      const step = Math.min(len, temperature) / len;
      xs[i] += forces.dx[i] * step;
      ys[i] += forces.dy[i] * step;
    }
    temperature = Math.max(2, temperature * 0.985);
  }

  ids.forEach((id, i) => placement.set(id, { x: xs[i], y: ys[i] }));
  resolveOverlaps(placement, s.nodes, Math.min(opts.gapX, opts.gapY));
  return placement;
}

type Forces = { dx: Float64Array; dy: Float64Array };

/** Push two nodes apart, harder the closer they are, up to `cutoff`. */
function push(xs: number[], ys: number[], { dx, dy }: Forces, i: number, j: number, k: number) {
  const vx = xs[i] - xs[j];
  const vy = ys[i] - ys[j];
  const dist = Math.max(1, Math.hypot(vx, vy));
  if (dist > 4 * k) return;
  const f = (k * k) / dist / dist;
  dx[i] += vx * f;
  dy[i] += vy * f;
  dx[j] -= vx * f;
  dy[j] -= vy * f;
}

/**
 * Nearby pairs push apart; everyone is pulled towards the middle. Nodes are
 * bucketed into cells as wide as the repulsion reach, so each node only
 * meets the nodes in its own and the neighbouring cells.
 */
function repel(xs: number[], ys: number[], forces: Forces, k: number) {
  const n = xs.length;
  const cell = 4 * k;
  const key = (cx: number, cy: number) => `${cx},${cy}`;
  const cells = new Map<string, number[]>();
  for (let i = 0; i < n; i++) {
    const id = key(Math.floor(xs[i] / cell), Math.floor(ys[i] / cell));
    const members = cells.get(id);
    if (members) members.push(i);
    else cells.set(id, [i]);
  }

  const OFFSETS = [-1, 0, 1].flatMap((ox) => [-1, 0, 1].map((oy) => [ox, oy] as const));
  for (const [id, members] of cells) {
    const [cx, cy] = id.split(',').map(Number);
    const nearby = OFFSETS.flatMap(([ox, oy]) => cells.get(key(cx + ox, cy + oy)) ?? []);
    for (const i of members) {
      for (const j of nearby.filter((j) => j > i)) push(xs, ys, forces, i, j, k);
    }
  }

  const mx = xs.reduce((sum, x) => sum + x, 0) / n;
  const my = ys.reduce((sum, y) => sum + y, 0) / n;
  for (let i = 0; i < n; i++) {
    forces.dx[i] += (mx - xs[i]) * GRAVITY;
    forces.dy[i] += (my - ys[i]) * GRAVITY;
  }
}

/** Edges are springs that want to be `k` long. */
function attract(xs: number[], ys: number[], { dx, dy }: Forces, springs: Array<[number, number]>, k: number) {
  for (const [a, b] of springs) {
    const vx = xs[a] - xs[b];
    const vy = ys[a] - ys[b];
    const f = Math.max(1, Math.hypot(vx, vy)) / k;
    dx[a] -= vx * f;
    dy[a] -= vy * f;
    dx[b] += vx * f;
    dy[b] += vy * f;
  }
}
