import { describe, expect, it } from 'vitest';

import { fitRow } from './layered';
import { computeLayout, type LayoutEdge, type LayoutNode } from './index';

const W = 170;
const H = 120;

function node(id: string, x = 0, y = 0): LayoutNode {
  return { id, x, y, width: W, height: H };
}

function chain(prefix: string, length: number, x = 0) {
  const nodes = Array.from({ length }, (_, i) => node(`${prefix}${i}`, x, i));
  const edges = nodes.slice(1).map((n, i) => ({ source: nodes[i].id, target: n.id }));
  return { nodes, edges };
}

function overlaps(pos: Map<string, { x: number; y: number }>) {
  const boxes = [...pos.entries()];
  const hits: string[] = [];
  for (let i = 0; i < boxes.length; i++) {
    for (let j = i + 1; j < boxes.length; j++) {
      const [a, pa] = boxes[i];
      const [b, pb] = boxes[j];
      if (Math.abs(pa.x - pb.x) < W && Math.abs(pa.y - pb.y) < H) hits.push(`${a}/${b}`);
    }
  }
  return hits;
}

/** Two trees joined by a Synthesizer node, a long thread, a cycle, loose nodes and a dangling edge. */
function mixedGraph() {
  const left = chain('l', 6);
  const right = chain('r', 4);
  const deep = chain('d', 25);
  const nodes: LayoutNode[] = [
    ...left.nodes,
    ...right.nodes,
    ...deep.nodes,
    node('m'),
    node('c1'),
    node('c2'),
    node('solo'),
    node('l2b'),
  ];
  const edges: LayoutEdge[] = [
    ...left.edges,
    ...right.edges,
    ...deep.edges,
    { source: 'l5', target: 'm' },
    { source: 'r3', target: 'm' },
    { source: 'c1', target: 'c2' },
    { source: 'c2', target: 'c1' },
    { source: 'l2', target: 'l2b' },
    { source: 'ghost', target: 'l0' },
  ];
  return { nodes, edges };
}

describe('computeLayout', () => {
  it('returns nothing for an empty graph', () => {
    expect(computeLayout([], [])).toEqual(new Map());
  });

  it('places every node, without overlaps, on the grid, despite cycles and missing endpoints', () => {
    const { nodes, edges } = mixedGraph();
    const pos = computeLayout(nodes, edges);

    expect([...pos.keys()].sort()).toEqual(nodes.map((n) => n.id).sort());
    expect(overlaps(pos)).toEqual([]);
    for (const p of pos.values()) {
      expect(p.x % 20).toBe(0);
      expect(p.y % 20).toBe(0);
    }
    // The cycle still reads top-down: one node of it sits above the other.
    expect(pos.get('c1')!.y).not.toBe(pos.get('c2')!.y);
  });

  it('is deterministic', () => {
    const { nodes, edges } = mixedGraph();
    expect(computeLayout(nodes, edges)).toEqual(computeLayout(nodes, edges));
  });

  it('anchors the result at the old top-left corner', () => {
    const { nodes, edges } = chain('a', 3);
    const moved = nodes.map((n, i) => ({ ...n, x: 400 + i * 7, y: 300 + i * 50 }));
    const pos = computeLayout(moved, edges);
    expect(Math.min(...[...pos.values()].map((p) => p.x))).toBe(400);
    expect(Math.min(...[...pos.values()].map((p) => p.y))).toBe(300);
  });

  it('stacks a single child directly below its parent', () => {
    const { nodes, edges } = chain('a', 4);
    const pos = computeLayout(nodes, edges, { gridSize: 0 });
    const xs = new Set([...pos.values()].map((p) => p.x));
    expect(xs.size).toBe(1);
    expect(pos.get('a1')!.y - pos.get('a0')!.y).toBe(H + 40);
  });

  it('centres a parent over its children, which keep their current left-to-right order', () => {
    const nodes = [node('p'), node('a', 20), node('b', -50), node('c', 10)];
    const edges = ['a', 'b', 'c'].map((t) => ({ source: 'p', target: t }));
    const pos = computeLayout(nodes, edges, { gridSize: 0 });
    expect(pos.get('p')!.x).toBeCloseTo(pos.get('c')!.x);
    expect(pos.get('b')!.x).toBeLessThan(pos.get('c')!.x);
    expect(pos.get('c')!.x).toBeLessThan(pos.get('a')!.x);
  });

  it('swaps siblings only where the swap removes a crossing', () => {
    // p's children start as [b, a]. a and q feed one Synthesizer node, and q
    // sits left, so keeping b first would cross b -> bc with a -> m.
    const nodes = [node('q', 0, 160), node('p', 300), node('b', 200, 160), node('a', 400, 160), node('bc', 200, 320), node('m', 100, 320)];
    const edges = [
      { source: 'p', target: 'b' },
      { source: 'p', target: 'a' },
      { source: 'b', target: 'bc' },
      { source: 'q', target: 'm' },
      { source: 'a', target: 'm' },
    ];
    const pos = computeLayout(nodes, edges, { gridSize: 0 });
    expect(pos.get('a')!.x).toBeLessThan(pos.get('b')!.x);
    expect(pos.get('q')!.x).toBeLessThan(pos.get('a')!.x);
  });

  it('puts a Synthesizer node below all of its parents and between them', () => {
    const left = chain('l', 4);
    const right = chain('r', 2, 500);
    const nodes = [...left.nodes, ...right.nodes, node('m')];
    const edges = [...left.edges, ...right.edges, { source: 'l3', target: 'm' }, { source: 'r1', target: 'm' }];
    const pos = computeLayout(nodes, edges, { gridSize: 0 });

    const m = pos.get('m')!;
    expect(m.y).toBeGreaterThan(pos.get('l3')!.y);
    expect(m.y).toBeGreaterThan(pos.get('r1')!.y);
    const [lo, hi] = [pos.get('l3')!.x, pos.get('r1')!.x].sort((a, b) => a - b);
    expect(m.x).toBeGreaterThanOrEqual(lo);
    expect(m.x).toBeLessThanOrEqual(hi);
    // The short tree is pulled down to join where it merges.
    expect(pos.get('r1')!.y).toBe(pos.get('l3')!.y);
  });

  it('keeps Graph components joined by a Synthesizer node next to each other', () => {
    // Two linked trees plus ten unrelated ones that could separate them.
    const a = chain('a', 3);
    const b = chain('b', 3);
    const others = Array.from({ length: 10 }, (_, i) => chain(`o${i}x`, 3));
    const nodes = [...a.nodes, ...others.flatMap((o) => o.nodes), ...b.nodes, node('m')];
    const edges = [
      ...a.edges,
      ...b.edges,
      ...others.flatMap((o) => o.edges),
      { source: 'a2', target: 'm' },
      { source: 'b2', target: 'm' },
    ];
    const pos = computeLayout(nodes, edges);
    // Side by side on the same layer, with no unrelated tree between them.
    expect(pos.get('a2')!.y).toBe(pos.get('b2')!.y);
    expect(Math.abs(pos.get('a2')!.x - pos.get('b2')!.x)).toBeLessThanOrEqual(W + 60);
  });

  it('packs many separate Graph components into a block, not one long row', () => {
    const trees = Array.from({ length: 16 }, (_, i) => chain(`t${i}x`, 3, i * 210));
    const pos = computeLayout(
      trees.flatMap((t) => t.nodes),
      trees.flatMap((t) => t.edges)
    );
    const xs = [...pos.values()].map((p) => p.x);
    const ys = [...pos.values()].map((p) => p.y);
    const width = Math.max(...xs) - Math.min(...xs) + W;
    const height = Math.max(...ys) - Math.min(...ys) + H;
    expect(width / height).toBeLessThan(3);
    expect(new Set(ys).size).toBeGreaterThan(3);
  });
});

describe('fitRow', () => {
  it('keeps targets that already respect the gaps', () => {
    expect(fitRow([0, 100, 300], [1, 1, 1], [50, 50])).toEqual([0, 100, 300]);
  });

  it('spreads clashing targets evenly around their weighted mean', () => {
    expect(fitRow([100, 100], [1, 1], [200])).toEqual([0, 200]);
    expect(fitRow([0, 0], [3, 1], [100])).toEqual([-25, 75]);
  });
});
