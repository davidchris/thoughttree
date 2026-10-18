import type { Placement } from './geometry';
import type { Structure } from './structure';

/**
 * Layered DAG layout (a small Sugiyama): every parent counts, not just the
 * first one, so a merge node sits below all of its parents, between them.
 *
 * 1. Layer = longest path from a source; then every node is pulled down to
 *    sit just above its nearest child, so a second tree joins where it merges.
 * 2. Edges that skip layers get invisible waypoints that reserve a lane.
 * 3. Barycenter sweeps reorder each layer to cut crossings.
 * 4. x is solved per layer as a weighted least-squares fit to the neighbours,
 *    with order and spacing as hard constraints (pool adjacent violators).
 */

const WAYPOINT_WIDTH = 24;
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
  for (const v of preorder(component, vertices)) layers[v.layer].push(v);
  return layers;
}

/** Depth-first order from the sources; for a pure tree this already has no crossings. */
function preorder(component: string[], vertices: Map<string, Vertex>): Vertex[] {
  const seen = new Set<Vertex>();
  const out: Vertex[] = [];
  const sources = component.map((id) => vertices.get(id)!).filter((v) => v.up.length === 0);
  const stack = [...sources].reverse();
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

/** Crossings between one layer and the next: pairs of edges whose ends swap order. */
function layerCrossings(layer: Vertex[]): number {
  const ends = layer.flatMap((v) => v.down.map((d) => [v.order, d.order] as const));
  let total = 0;
  ends.forEach(([a1, b1], i) => {
    for (const [a2, b2] of ends.slice(i + 1)) if ((a1 - a2) * (b1 - b2) < 0) total++;
  });
  return total;
}

function crossings(layers: Vertex[][]): number {
  return layers.reduce((sum, layer) => sum + layerCrossings(layer), 0);
}

function sortByBarycenter(layer: Vertex[], neighbours: (v: Vertex) => Vertex[]) {
  const key = new Map<Vertex, number>();
  for (const v of layer) {
    const ns = neighbours(v);
    key.set(v, ns.length ? ns.reduce((sum, n) => sum + n.order, 0) / ns.length : v.order);
  }
  layer.sort((a, b) => key.get(a)! - key.get(b)! || a.order - b.order);
  layer.forEach((v, i) => (v.order = i));
}

function reduceCrossings(layers: Vertex[][]) {
  renumber(layers);
  let best = layers.map((l) => [...l]);
  let bestCrossings = crossings(layers);

  for (let sweep = 0; sweep < SWEEPS && bestCrossings > 0; sweep++) {
    for (let l = 1; l < layers.length; l++) sortByBarycenter(layers[l], (v) => v.up);
    for (let l = layers.length - 2; l >= 0; l--) sortByBarycenter(layers[l], (v) => v.down);
    const c = crossings(layers);
    if (c < bestCrossings) {
      bestCrossings = c;
      best = layers.map((layer) => [...layer]);
    }
  }
  best.forEach((layer, l) => (layers[l] = layer));
  renumber(layers);
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

function layout(s: Structure, component: string[], gapX: number): Vertex[][] {
  const layers = buildLayers(s, component, assignLayers(s, component));
  reduceCrossings(layers);
  assignX(layers, gapX);
  return layers;
}

export function layeredComponent(
  s: Structure,
  component: string[],
  { gapX, gapY }: LayeredOptions
): Placement {
  const placement: Placement = new Map();
  let top = 0;
  for (const layer of layout(s, component, gapX)) {
    const height = Math.max(0, ...layer.map((v) => v.height));
    for (const v of layer) {
      if (s.nodes.has(v.id)) placement.set(v.id, { x: v.x, y: top + height / 2 });
    }
    top += height + gapY;
  }
  return placement;
}

/** Target width:height of a folded conversation, close to a laptop screen. */
const FOLD_ASPECT = 1.6;
/** Short threads stay straight: a band holds at least this many layers. */
const MIN_BAND_ROWS = 6;

interface Band {
  first: number;
  end: number; // exclusive
  min: number;
  width: number;
}

/** A fold may cut between two layers only if each holds at most this many nodes. */
const QUIET_CUT = 2;

/**
 * Where the band starting at `first` should end. The turn edges of a fold run
 * along the row shared by the two layers around the cut, so the cut goes
 * where those layers are emptiest, preferring the last half of the band. When
 * no quiet cut is in reach the band runs on to the next quiet one: a band
 * that is too tall reads better than turn edges running through nodes.
 */
function bandEnd(layers: Vertex[][], first: number, rows: number): number {
  const crowd = (e: number) => Math.max(layers[e - 1].length, layers[e].length);
  let end = first + rows;
  if (end >= layers.length) return layers.length;
  for (let e = end - 1; e >= first + Math.ceil(rows / 2); e--) {
    if (crowd(e) < crowd(end)) end = e;
  }
  while (end < layers.length && crowd(end) > QUIET_CUT) end++;
  return end;
}

/** Cut the layers into bands of about `rows` layers each. */
function cutBands(layers: Vertex[][], rows: number): Band[] {
  const bands: Band[] = [];
  for (let first = 0; first < layers.length; ) {
    const end = bandEnd(layers, first, rows);
    const slice = layers.slice(first, end).flat();
    const min = Math.min(...slice.map((v) => v.x - v.width / 2));
    const max = Math.max(...slice.map((v) => v.x + v.width / 2));
    bands.push({ first, end, min, width: max - min });
    first = end;
  }
  return bands;
}

/**
 * Folded (serpentine) layout: the layered drawing cut into bands placed side
 * by side. Even bands read downwards, odd bands upwards, so a long thread
 * snakes down, steps right, climbs back up, and so on. The band height is
 * chosen so the result is about as wide as it is tall on a 16:10 screen;
 * shallow conversations stay unfolded.
 */
export function foldedComponent(
  s: Structure,
  component: string[],
  { gapX, gapY }: LayeredOptions
): Placement {
  const layers = layout(s, component, gapX);
  const pitch = Math.max(...component.map((id) => s.nodes.get(id)!.height)) + gapY;
  const bandGap = gapX * 2;

  const score = (bands: Band[]) => {
    const width = bands.reduce((sum, b) => sum + b.width, 0) + (bands.length - 1) * bandGap;
    const height = Math.max(...bands.map((b) => b.end - b.first)) * pitch;
    return Math.max(width / FOLD_ASPECT, height);
  };

  // Ties go to taller bands: fewer folds read more easily.
  let bands = cutBands(layers, layers.length);
  for (let rows = layers.length - 1; rows >= MIN_BAND_ROWS; rows--) {
    const candidate = cutBands(layers, rows);
    if (score(candidate) < score(bands)) bands = candidate;
  }

  // Odd bands are turned 180 degrees, like folded paper: each band starts on
  // the row where the previous one ended, and parallel threads nest instead
  // of crossing at the turn.
  const placement: Placement = new Map();
  let left = 0;
  let row = 0;
  bands.forEach((band, b) => {
    const step = b % 2 === 0 ? 1 : -1;
    const x = (v: Vertex) => left + (step > 0 ? v.x - band.min : band.min + band.width - v.x);
    for (let l = band.first; l < band.end; l++, row += step) {
      for (const v of layers[l]) {
        if (s.nodes.has(v.id)) placement.set(v.id, { x: x(v), y: row * pitch });
      }
    }
    row -= step;
    left += band.width + bandGap;
  });
  return placement;
}
