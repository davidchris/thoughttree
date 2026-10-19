import type { Placement } from './geometry';
import { byPosition, type Structure } from './structure';

/**
 * Layered DAG layout (a small Sugiyama): every parent counts, not just the
 * first one, so a Synthesizer node sits below all of its parents, between them.
 *
 * 1. Layer = longest path from a source; then every node is pulled down to
 *    sit just above its nearest child, so a second tree joins where it merges.
 * 2. Edges that skip layers get invisible waypoints that reserve a lane.
 * 3. Layers start in depth-first order from the sources, left to right by
 *    current position. Neighbours swap only where that removes crossings, so
 *    siblings keep their current order unless a swap removes a crossing.
 * 4. x is solved per layer as a weighted least-squares fit to the neighbours,
 *    with order and spacing as hard constraints (pool adjacent violators).
 */

const WAYPOINT_WIDTH = 24;
/** Alternating up/down passes of the x fit. */
const SWEEPS = 8;

interface Vertex {
  id: string;
  layer: number;
  width: number;
  height: number;
  up: Vertex[];
  down: Vertex[];
  x: number;
  order: number;
}

export interface LayeredOptions {
  gapX: number;
  gapY: number;
}

function assignLayers(s: Structure, component: string[]): Map<string, number> {
  const inDegree = new Map(component.map((id) => [id, s.parents.get(id)?.length ?? 0] as const));
  const layer = new Map<string, number>();
  const queue = component.filter((id) => inDegree.get(id) === 0);
  for (const id of queue) layer.set(id, 0);

  for (let i = 0; i < queue.length; i++) {
    const id = queue[i];
    for (const kid of s.children.get(id) ?? []) {
      layer.set(kid, Math.max(layer.get(kid) ?? 0, layer.get(id)! + 1));
      inDegree.set(kid, inDegree.get(kid)! - 1);
      if (inDegree.get(kid) === 0) queue.push(kid);
    }
  }

  // Pull nodes down to sit just above their nearest child. Only nodes that
  // feed a deeper merge move, so a second tree joins where it merges.
  for (let i = queue.length - 1; i >= 0; i--) {
    const kids = s.children.get(queue[i]);
    if (kids?.length) layer.set(queue[i], Math.min(...kids.map((k) => layer.get(k)!)) - 1);
  }
  return layer;
}

/** Real vertices plus waypoints on long edges, grouped by layer. */
function buildLayers(s: Structure, component: string[], layerOf: Map<string, number>) {
  const minLayer = Math.min(...component.map((id) => layerOf.get(id)!));
  const vertices = new Map<string, Vertex>();
  for (const id of component) {
    const n = s.nodes.get(id)!;
    vertices.set(id, {
      id,
      layer: layerOf.get(id)! - minLayer,
      width: n.width,
      height: n.height,
      up: [],
      down: [],
      x: 0,
      order: 0,
    });
  }

  const all = [...vertices.values()];
  for (const id of component) {
    for (const kid of s.children.get(id) ?? []) {
      let from = vertices.get(id)!;
      const to = vertices.get(kid)!;
      for (let l = from.layer + 1; l < to.layer; l++) {
        const waypoint: Vertex = {
          id: `${id}\u0000${kid}\u0000${l}`,
          layer: l,
          width: WAYPOINT_WIDTH,
          height: 0,
          up: [from],
          down: [],
          x: 0,
          order: 0,
        };
        from.down.push(waypoint);
        all.push(waypoint);
        from = waypoint;
      }
      from.down.push(to);
      to.up.push(from);
    }
  }

  const depth = Math.max(0, ...all.map((v) => v.layer)) + 1;
  const layers: Vertex[][] = Array.from({ length: depth }, () => []);
  for (const v of preorder(s, component, vertices)) layers[v.layer].push(v);
  return layers;
}

/** Depth-first order from the sources; for a pure tree this already has no crossings. */
function preorder(s: Structure, component: string[], vertices: Map<string, Vertex>): Vertex[] {
  const seen = new Set<Vertex>();
  const out: Vertex[] = [];
  const sources = component.filter((id) => !s.parents.has(id)).sort(byPosition(s.nodes));
  const stack = sources.map((id) => vertices.get(id)!).reverse();
  while (stack.length) {
    const v = stack.pop()!;
    if (seen.has(v)) continue;
    seen.add(v);
    out.push(v);
    for (let i = v.down.length - 1; i >= 0; i--) stack.push(v.down[i]);
  }
  return out;
}

function renumber(layers: Vertex[][]) {
  for (const layer of layers) layer.forEach((v, i) => (v.order = i));
}

/** Crossings among the edges of u and v toward one neighbouring layer, with u left of v. */
function pairCrossings(uEnds: Vertex[], vEnds: Vertex[]): number {
  let total = 0;
  for (const a of uEnds) for (const b of vEnds) if (a.order > b.order) total++;
  return total;
}

/**
 * Swap neighbours in a layer only where the swap removes crossings, until no
 * such swap is left. Every swap lowers the total, so this ends, and two
 * vertices never trade places for nothing.
 */
function reduceCrossings(layers: Vertex[][]) {
  renumber(layers);
  for (let swapped = true; swapped; ) {
    swapped = false;
    for (const layer of layers) swapped = swapPass(layer) || swapped;
  }
}

/** One left-to-right pass over a layer. Returns true when anything swapped. */
function swapPass(layer: Vertex[]): boolean {
  let swapped = false;
  for (let i = 0; i + 1 < layer.length; i++) {
    const [u, v] = [layer[i], layer[i + 1]];
    const kept = pairCrossings(u.up, v.up) + pairCrossings(u.down, v.down);
    const flipped = pairCrossings(v.up, u.up) + pairCrossings(v.down, u.down);
    if (flipped >= kept) continue;
    [layer[i], layer[i + 1]] = [v, u];
    [u.order, v.order] = [i + 1, i];
    swapped = true;
  }
  return swapped;
}

/**
 * Weighted least-squares placement of an ordered row: minimise
 * sum(w * (x - target)^2) subject to x[i+1] - x[i] >= gap[i].
 * Substituting y = x - offset turns this into isotonic regression (PAV).
 */
export function fitRow(targets: number[], weights: number[], gaps: number[]): number[] {
  const offset = [0];
  for (let i = 1; i < targets.length; i++) offset.push(offset[i - 1] + gaps[i - 1]);

  const blocks: Array<{ w: number; sum: number; size: number }> = [];
  targets.forEach((t, i) => {
    blocks.push({ w: weights[i], sum: weights[i] * (t - offset[i]), size: 1 });
    while (blocks.length > 1) {
      const b = blocks[blocks.length - 1];
      const a = blocks[blocks.length - 2];
      if (a.sum / a.w <= b.sum / b.w) break;
      blocks.splice(-2, 2, { w: a.w + b.w, sum: a.sum + b.sum, size: a.size + b.size });
    }
  });

  const out: number[] = [];
  for (const b of blocks) {
    for (let k = 0; k < b.size; k++) out.push(b.sum / b.w + offset[out.length]);
  }
  return out;
}

function alignLayer(layer: Vertex[], neighbours: (v: Vertex) => Vertex[], gapX: number) {
  const targets = layer.map((v) => {
    const ns = neighbours(v);
    return ns.length ? ns.reduce((sum, n) => sum + n.x, 0) / ns.length : v.x;
  });
  const weights = layer.map((v) => Math.max(1, neighbours(v).length));
  const gaps = layer.slice(1).map((v, i) => (layer[i].width + v.width) / 2 + gapX);
  fitRow(targets, weights, gaps).forEach((x, i) => (layer[i].x = x));
}

function assignX(layers: Vertex[][], gapX: number) {
  for (const layer of layers) {
    let x = 0;
    layer.forEach((v, i) => {
      if (i > 0) x += (layer[i - 1].width + v.width) / 2 + gapX;
      v.x = x;
    });
  }
  for (let round = 0; round < SWEEPS; round++) {
    for (let l = layers.length - 2; l >= 0; l--) alignLayer(layers[l], (v) => v.down, gapX);
    for (let l = 1; l < layers.length; l++) alignLayer(layers[l], (v) => v.up, gapX);
  }
}

/** Node centres of one Graph component, with layer 0 starting at y = 0. */
export function layeredComponent(
  s: Structure,
  component: string[],
  { gapX, gapY }: LayeredOptions
): Placement {
  const layers = buildLayers(s, component, assignLayers(s, component));
  reduceCrossings(layers);
  assignX(layers, gapX);

  const placement: Placement = new Map();
  let top = 0;
  for (const layer of layers) {
    const height = Math.max(0, ...layer.map((v) => v.height));
    for (const v of layer) {
      if (s.nodes.has(v.id)) placement.set(v.id, { x: v.x, y: top + height / 2 });
    }
    top += height + gapY;
  }
  return placement;
}
