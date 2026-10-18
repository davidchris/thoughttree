import { describe, expect, it } from 'vitest';

import { fitRow } from './layered';
import { computeStyledLayout, LAYOUT_STYLES, type LayoutEdge, type LayoutNode, type LayoutStyle } from './index';

const W = 170;
const H = 120;
const STYLES = LAYOUT_STYLES.map((s) => s.id).filter((s): s is Exclude<LayoutStyle, 'tree'> => s !== 'tree');

function node(id: string, x = 0, y = 0): LayoutNode {
  return { id, x, y, width: W, height: H };
}

function chain(prefix: string, length: number) {
  const nodes = Array.from({ length }, (_, i) => node(`${prefix}${i}`, 0, i));
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

/** Two trees joined by a merge, a long thread, a cycle and a few loose nodes. */
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

describe.each(STYLES)('%s layout', (style) => {
  it('places every node, without overlaps, on the grid', () => {
    const { nodes, edges } = mixedGraph();
    const pos = computeStyledLayout(style, nodes, edges);

    expect([...pos.keys()].sort()).toEqual(nodes.map((n) => n.id).sort());
    expect(overlaps(pos)).toEqual([]);
    for (const p of pos.values()) {
      expect(p.x % 20).toBe(0);
      expect(p.y % 20).toBe(0);
    }
  });

  it('is deterministic', () => {
    const { nodes, edges } = mixedGraph();
    expect(computeStyledLayout(style, nodes, edges)).toEqual(computeStyledLayout(style, nodes, edges));
  });

  it('anchors the result at the old top-left corner', () => {
    const { nodes, edges } = chain('a', 3);
    const moved = nodes.map((n, i) => ({ ...n, x: 400 + i * 7, y: 300 + i * 50 }));
    const pos = computeStyledLayout(style, moved, edges);
    expect(Math.min(...[...pos.values()].map((p) => p.x))).toBe(400);
    expect(Math.min(...[...pos.values()].map((p) => p.y))).toBe(300);
  });

  it('keeps trees joined by a merge next to each other', () => {
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
    const pos = computeStyledLayout(style, nodes, edges);
    // The merge's parents end up close, with no unrelated tree between them.
    const gap = Math.hypot(pos.get('a2')!.x - pos.get('b2')!.x, pos.get('a2')!.y - pos.get('b2')!.y);
    expect(gap).toBeLessThan(3 * W);
  });
});

describe('layered layout', () => {
  it('stacks a single child directly below its parent', () => {
    const { nodes, edges } = chain('a', 4);
    const pos = computeStyledLayout('layered', nodes, edges, { gridSize: 0 });
    const xs = new Set([...pos.values()].map((p) => p.x));
    expect(xs.size).toBe(1);
    expect(pos.get('a1')!.y - pos.get('a0')!.y).toBe(H + 40);
  });

  it('centres a parent over its children', () => {
    const nodes = [node('p'), node('a', 0), node('b', 10), node('c', 20)];
    const edges = ['a', 'b', 'c'].map((t) => ({ source: 'p', target: t }));
    const pos = computeStyledLayout('layered', nodes, edges, { gridSize: 0 });
    expect(pos.get('p')!.x).toBeCloseTo(pos.get('b')!.x);
    expect(pos.get('a')!.x).toBeLessThan(pos.get('b')!.x);
    expect(pos.get('b')!.x).toBeLessThan(pos.get('c')!.x);
  });

  it('puts a merge below all of its parents and between them', () => {
    const left = chain('l', 4);
    const right = chain('r', 2);
    const nodes = [...left.nodes, ...right.nodes.map((n) => ({ ...n, x: 500 })), node('m')];
    const edges = [...left.edges, ...right.edges, { source: 'l3', target: 'm' }, { source: 'r1', target: 'm' }];
    const pos = computeStyledLayout('layered', nodes, edges, { gridSize: 0 });

    const m = pos.get('m')!;
    expect(m.y).toBeGreaterThan(pos.get('l3')!.y);
    expect(m.y).toBeGreaterThan(pos.get('r1')!.y);
    const [lo, hi] = [pos.get('l3')!.x, pos.get('r1')!.x].sort((a, b) => a - b);
    expect(m.x).toBeGreaterThanOrEqual(lo);
    expect(m.x).toBeLessThanOrEqual(hi);
    // The short tree is pulled down to join where it merges.
    expect(pos.get('r1')!.y).toBe(pos.get('l3')!.y);
  });

  it('packs many separate conversations into a block, not one long row', () => {
    const trees = Array.from({ length: 16 }, (_, i) => chain(`t${i}x`, 3));
    const pos = computeStyledLayout(
      'layered',
      trees.flatMap((t) => t.nodes),
      trees.flatMap((t) => t.edges)
    );
    const xs = [...pos.values()].map((p) => p.x);
    const ys = [...pos.values()].map((p) => p.y);
    const width = Math.max(...xs) - Math.min(...xs) + W;
    const height = Math.max(...ys) - Math.min(...ys) + H;
    expect(width / height).toBeLessThan(3);
  });
});

describe('folded layout', () => {
  it('leaves a shallow conversation unfolded', () => {
    const { nodes, edges } = chain('a', 4);
    expect(computeStyledLayout('folded', nodes, edges)).toEqual(computeStyledLayout('layered', nodes, edges));
  });

  it('folds a long thread into columns that alternate down and up', () => {
    const { nodes, edges } = chain('a', 40);
    const pos = computeStyledLayout('folded', nodes, edges, { gridSize: 0 });
    const ys = [...pos.values()].map((p) => p.y);
    expect(Math.max(...ys) - Math.min(...ys)).toBeLessThan(20 * (H + 40));

    // Every step is one row up or down, or one level step sideways at a turn.
    let ups = 0;
    for (let i = 1; i < 40; i++) {
      const a = pos.get(`a${i - 1}`)!;
      const b = pos.get(`a${i}`)!;
      if (a.y === b.y) expect(b.x).toBeGreaterThan(a.x);
      else expect(Math.abs(b.y - a.y)).toBe(H + 40);
      if (b.y < a.y) ups++;
    }
    expect(ups).toBeGreaterThan(0);
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
