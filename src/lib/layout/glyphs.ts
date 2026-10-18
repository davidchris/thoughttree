import type { LayoutStyle } from './index';

/**
 * 16px pictograms of each tidy style, drawn in the app icon's grammar: the
 * root is a filled dot, every other node an outlined one, links are plain
 * strokes. Shared by the toolbar (React) and the layout gallery (plain SVG).
 */
export interface Glyph {
  /** Polylines as flat [x1, y1, x2, y2, ...] lists. */
  links: number[][];
  /** [cx, cy]; the first node is the root and is drawn filled. */
  nodes: Array<[number, number]>;
}

export const NODE_RADIUS = 1.75;

export const LAYOUT_GLYPHS: Record<LayoutStyle, Glyph> = {
  // Two trees side by side, each growing down.
  tree: {
    links: [
      [4, 2.5, 4, 13.5],
      [12, 2.5, 12, 8, 9.5, 13.5],
      [12, 8, 14.5, 13.5],
    ],
    nodes: [[4, 2.5], [4, 8], [4, 13.5], [12, 2.5], [12, 8], [9.5, 13.5], [14.5, 13.5]],
  },
  // Two threads meeting in a merge that sits between them.
  layered: {
    links: [
      [4, 2.5, 4, 8, 8, 13.5],
      [12, 2.5, 12, 8, 8, 13.5],
    ],
    nodes: [[4, 2.5], [4, 8], [12, 2.5], [12, 8], [8, 13.5]],
  },
  // One thread snaking down, across, up, across, down.
  folded: {
    links: [[3, 2.5, 3, 13.5, 8, 13.5, 8, 2.5, 13, 2.5, 13, 8]],
    nodes: [[3, 2.5], [3, 8], [3, 13.5], [8, 13.5], [8, 8], [8, 2.5], [13, 2.5], [13, 8]],
  },
  // A middle node with spokes out to a ring.
  radial: {
    links: [
      [8, 8, 8, 2.5],
      [8, 8, 13, 10.5],
      [8, 8, 3, 10.5],
      [13, 10.5, 12, 14],
    ],
    nodes: [[8, 8], [8, 2.5], [13, 10.5], [3, 10.5], [12, 14]],
  },
  // An irregular network.
  organic: {
    links: [
      [5, 4, 11, 2.5, 13.5, 8, 10, 13],
      [5, 4, 3, 10, 6, 14],
      [11, 2.5, 10, 13],
    ],
    nodes: [[5, 4], [11, 2.5], [13.5, 8], [10, 13], [3, 10], [6, 14]],
  },
};

export const LINK_WIDTH = 1.25;

let maskCount = 0;

/**
 * The glyph as SVG markup, for code outside React. Links are masked out
 * under each node, so outlined nodes stay hollow on any background.
 */
export function glyphSvg(style: LayoutStyle, size = 16): string {
  const { links, nodes } = LAYOUT_GLYPHS[style];
  const mask = `tt-glyph-${style}-${maskCount++}`;
  const holes = nodes.map(([cx, cy]) => `<circle cx="${cx}" cy="${cy}" r="${NODE_RADIUS + 0.4}" fill="black" stroke="none"/>`).join('');
  const lines = links.map((l) => `<polyline points="${l.join(' ')}"/>`).join('');
  const dots = nodes
    .map(([cx, cy], i) => `<circle cx="${cx}" cy="${cy}" r="${NODE_RADIUS}"${i === 0 ? ' fill="currentColor"' : ''}/>`)
    .join('');
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 16 16" aria-hidden="true" ` +
    `fill="none" stroke="currentColor" stroke-width="${LINK_WIDTH}" stroke-linecap="round" stroke-linejoin="round">` +
    `<mask id="${mask}"><rect width="16" height="16" fill="white" stroke="none"/>${holes}</mask>` +
    `<g mask="url(#${mask})">${lines}</g>${dots}</svg>`
  );
}
