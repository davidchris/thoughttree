import { describe, expect, it } from 'vitest';
import type { Edge, Node } from '@xyflow/react';

import { computeAutoLayout } from './graphLayout';

function node(id: string, x = 0, y = 0): Node {
  return { id, position: { x, y }, data: {} };
}

function edge(source: string, target: string): Edge {
  return { id: `${source}->${target}`, source, target };
}

/** Layout with no grid snapping, so raw slot maths stays visible in assertions. */
const raw = { gridSize: 0 };

describe('computeAutoLayout', () => {
  it('returns an empty map for an empty graph', () => {
    expect(computeAutoLayout([], [])).toEqual(new Map());
  });

  it('stacks a linear chain in a single column', () => {
    const nodes = [node('a'), node('b', 500, 500), node('c', -300, 90)];
    const edges = [edge('a', 'b'), edge('b', 'c')];

    const pos = computeAutoLayout(nodes, edges, raw);

    // Anchored at the current top-left of the selection (min x / min y).
    expect(pos.get('a')).toEqual({ x: -300, y: 0 });
    expect(pos.get('b')).toEqual({ x: -300, y: 160 });
    expect(pos.get('c')).toEqual({ x: -300, y: 320 });
  });

  it('spreads siblings horizontally and centres the parent above them', () => {
    const nodes = [node('root'), node('l', 10), node('r', 20)];
    const edges = [edge('root', 'l'), edge('root', 'r')];

    const pos = computeAutoLayout(nodes, edges, raw);

    expect(pos.get('l')).toEqual({ x: 0, y: 160 });
    expect(pos.get('r')).toEqual({ x: 210, y: 160 });
    expect(pos.get('root')).toEqual({ x: 105, y: 0 });
  });

  it('orders siblings by their current x position', () => {
    const nodes = [node('root'), node('l', 900), node('r', 5)];
    const edges = [edge('root', 'l'), edge('root', 'r')];

    const pos = computeAutoLayout(nodes, edges, raw);

    // 'r' sits further left today, so it keeps the left slot.
    expect(pos.get('r')!.x).toBeLessThan(pos.get('l')!.x);
  });

  it('separates independent trees by one extra slot', () => {
    const nodes = [node('r1'), node('r2', 50)];
    const pos = computeAutoLayout(nodes, [], raw);

    expect(pos.get('r1')).toEqual({ x: 0, y: 0 });
    expect(pos.get('r2')).toEqual({ x: 420, y: 0 });
  });

  it('honours custom gaps and layout direction', () => {
    const nodes = [node('root'), node('l'), node('r')];
    const edges = [edge('root', 'l'), edge('root', 'r')];

    const pos = computeAutoLayout(nodes, edges, {
      ...raw,
      direction: 'LR',
      nodeGap: 100,
      levelGap: 40,
    });

    // LR swaps the axes: depth runs along x, sibling spread along y.
    expect(pos.get('root')).toEqual({ x: 0, y: 50 });
    expect(pos.get('l')).toEqual({ x: 40, y: 0 });
    expect(pos.get('r')).toEqual({ x: 40, y: 100 });
  });

  it('snaps results to the grid', () => {
    const nodes = [node('root'), node('l'), node('r')];
    const edges = [edge('root', 'l'), edge('root', 'r')];

    const pos = computeAutoLayout(nodes, edges, { gridSize: 50 });

    // Parent centre is 105 unsnapped, children at 0 / 210.
    expect(pos.get('root')).toEqual({ x: 100, y: 0 });
    expect(pos.get('l')).toEqual({ x: 0, y: 150 });
    expect(pos.get('r')).toEqual({ x: 200, y: 150 });
  });

  it('ignores edges that reference unknown nodes', () => {
    const nodes = [node('a'), node('b', 400)];
    const edges = [edge('a', 'ghost'), edge('ghost', 'b')];

    const pos = computeAutoLayout(nodes, edges, raw);

    // Both stay roots, so they are laid out as two separate trees.
    expect(pos.get('a')).toEqual({ x: 0, y: 0 });
    expect(pos.get('b')).toEqual({ x: 420, y: 0 });
  });

  it('keeps the first incoming edge as the single parent', () => {
    const nodes = [node('p1'), node('p2', 300), node('c', 150, 200)];
    const edges = [edge('p1', 'c'), edge('p2', 'c')];

    const pos = computeAutoLayout(nodes, edges, raw);

    // 'c' hangs off p1 only, so it shares p1's column one level down.
    expect(pos.get('c')!.x).toBe(pos.get('p1')!.x);
    expect(pos.get('c')!.y).toBe(pos.get('p1')!.y + 160);
  });

  it('falls back to grid-snapped current positions for unplaceable nodes', () => {
    // 'a' and 'b' form a cycle that no root reaches, so they get neither a
    // depth nor a slot and keep their current position instead.
    const nodes = [node('root'), node('a', 11, 19), node('b', 47, 102)];
    const edges = [edge('a', 'b'), edge('b', 'a')];

    const pos = computeAutoLayout(nodes, edges, { gridSize: 20 });

    expect(pos.get('root')).toEqual({ x: 0, y: 0 });
    expect(pos.get('a')).toEqual({ x: 20, y: 20 });
    expect(pos.get('b')).toEqual({ x: 40, y: 100 });
  });

  it('lays out every node it is given', () => {
    const nodes = [node('a'), node('b'), node('c'), node('d')];
    const edges = [edge('a', 'b'), edge('a', 'c'), edge('c', 'd')];

    const pos = computeAutoLayout(nodes, edges);

    expect([...pos.keys()].sort()).toEqual(['a', 'b', 'c', 'd']);
  });
});
