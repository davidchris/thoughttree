import type { Edge, Node } from '@xyflow/react';

import { computeAutoLayout } from '../../src/lib/graphLayout';
import { edgeAnchors, type Rect, type Side } from '../../src/lib/graph/edgeAnchors';
import { LAYOUT_STYLES, type LayoutStyle } from '../../src/lib/layout';
import { glyphSvg } from '../../src/lib/layout/glyphs';

interface GalleryNode {
  id: string;
  role: string;
  label: string;
  x: number;
  y: number;
}

interface GalleryGraph {
  name: string;
  synthetic?: boolean;
  nodes: GalleryNode[];
  edges: Array<{ source: string; target: string }>;
}

const W = 170;
const H = 120;
const SVG_NS = 'http://www.w3.org/2000/svg';

// ---------- synthetic graphs ----------

function mulberry32(seed: number) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** One conversation: mostly continues the current thread, sometimes forks an earlier answer. */
function conversation(g: GalleryGraph, rng: () => number, prefix: string, size: number, forkChance: number) {
  const ids: string[] = [];
  const add = (role: string, parent?: string) => {
    const id = `${prefix}${ids.length}`;
    g.nodes.push({ id, role, label: `${prefix}${ids.length} ${role === 'user' ? 'question' : 'answer'}`, x: g.nodes.length, y: 0 });
    if (parent) g.edges.push({ source: parent, target: id });
    ids.push(id);
    return id;
  };
  let tip = add('user');
  while (ids.length < size) {
    const answers = ids.filter((id) => g.nodes.find((n) => n.id === id)!.role === 'assistant');
    if (answers.length > 1 && rng() < forkChance) {
      tip = add('user', answers[Math.floor(rng() * answers.length)]);
    } else {
      const role = g.nodes.find((n) => n.id === tip)!.role === 'user' ? 'assistant' : 'user';
      tip = add(role, tip);
    }
  }
  return ids;
}

function merge(g: GalleryGraph, id: string, parents: string[]) {
  g.nodes.push({ id, role: 'user', label: `${id}: merge`, x: g.nodes.length, y: 0 });
  for (const p of parents) g.edges.push({ source: p, target: id });
  g.nodes.push({ id: `${id}a`, role: 'assistant', label: `${id}: synthesis`, x: g.nodes.length, y: 0 });
  g.edges.push({ source: id, target: `${id}a` });
}

function researchDay(): GalleryGraph {
  const g: GalleryGraph = { name: 'Research day', synthetic: true, nodes: [], edges: [] };
  const rng = mulberry32(7);
  const a = conversation(g, rng, 'A', 34, 0.18);
  ['S', 'T', 'U', 'V', 'X', 'Y'].forEach((p, i) => conversation(g, rng, p, 2 + (i % 3) * 2, 0.3));
  const b = conversation(g, rng, 'B', 22, 0.2);
  const c = conversation(g, rng, 'C', 28, 0.15);
  merge(g, 'M1', [a[13], c[21]]);
  merge(g, 'M2', [b[9], c[5]]);
  return g;
}

function deepThread(): GalleryGraph {
  const g: GalleryGraph = { name: 'One deep thread', synthetic: true, nodes: [], edges: [] };
  conversation(g, mulberry32(3), 'D', 60, 0.08);
  return g;
}

// ---------- layout + metrics ----------

function layout(graph: GalleryGraph, style: LayoutStyle) {
  const nodes: Node[] = graph.nodes.map((n) => ({ id: n.id, position: { x: n.x, y: n.y }, data: {} }));
  const edges: Edge[] = graph.edges.map((e, i) => ({ id: `e${i}`, ...e }));
  return computeAutoLayout(nodes, edges, { style, gridSize: 20 });
}

function centre(p: { x: number; y: number }) {
  return { x: p.x + W / 2, y: p.y + H / 2 };
}

function segmentsCross(a: number[], b: number[]) {
  const orient = (px: number, py: number, qx: number, qy: number, rx: number, ry: number) =>
    Math.sign((qx - px) * (ry - py) - (qy - py) * (rx - px));
  return (
    orient(a[0], a[1], a[2], a[3], b[0], b[1]) * orient(a[0], a[1], a[2], a[3], b[2], b[3]) < 0 &&
    orient(b[0], b[1], b[2], b[3], a[0], a[1]) * orient(b[0], b[1], b[2], b[3], a[2], a[3]) < 0
  );
}

function segmentHitsBox(seg: number[], x0: number, y0: number, x1: number, y1: number) {
  // Liang-Barsky clip of the segment against the box.
  const [ax, ay, bx, by] = seg;
  const dx = bx - ax;
  const dy = by - ay;
  let t0 = 0;
  let t1 = 1;
  for (const [p, q] of [[-dx, ax - x0], [dx, x1 - ax], [-dy, ay - y0], [dy, y1 - ay]]) {
    if (p === 0) {
      if (q < 0) return false;
      continue;
    }
    const t = q / p;
    if (p < 0) t0 = Math.max(t0, t);
    else t1 = Math.min(t1, t);
    if (t0 > t1) return false;
  }
  return true;
}

function metrics(graph: GalleryGraph, pos: Map<string, { x: number; y: number }>) {
  const xs = [...pos.values()].map((p) => p.x);
  const ys = [...pos.values()].map((p) => p.y);
  const width = Math.max(...xs) - Math.min(...xs) + W;
  const height = Math.max(...ys) - Math.min(...ys) + H;
  const segs = graph.edges.map((e) => {
    const s = centre(pos.get(e.source)!);
    const t = centre(pos.get(e.target)!);
    return { ids: [e.source, e.target], seg: [s.x, s.y, t.x, t.y] };
  });
  const meanEdge = segs.reduce((sum, { seg }) => sum + Math.hypot(seg[2] - seg[0], seg[3] - seg[1]), 0) / Math.max(1, segs.length);
  let crossings = 0;
  for (let i = 0; i < segs.length; i++) {
    for (let j = i + 1; j < segs.length; j++) {
      if (segs[i].ids.some((id) => segs[j].ids.includes(id))) continue;
      if (segmentsCross(segs[i].seg, segs[j].seg)) crossings++;
    }
  }
  // Edges drawn straight through a node that is not one of their ends.
  let throughNodes = 0;
  for (const { ids, seg } of segs) {
    for (const [id, p] of pos) {
      if (ids.includes(id)) continue;
      if (segmentHitsBox(seg, p.x + 8, p.y + 8, p.x + W - 8, p.y + H - 8)) throughNodes++;
    }
  }
  // Zoom needed to see everything on a 1440x900 canvas; higher reads better.
  const fitZoom = Math.min(1, 1440 / width, 900 / height);
  return { width, height, meanEdge, crossings, throughNodes, fitZoom };
}

// ---------- rendering ----------

function lineageOf(graph: GalleryGraph, id: string | null): Set<string> {
  const out = new Set<string>();
  if (!id) return out;
  const parents = new Map<string, string[]>();
  for (const e of graph.edges) parents.set(e.target, [...(parents.get(e.target) ?? []), e.source]);
  const stack = [id];
  while (stack.length) {
    for (const p of parents.get(stack.pop()!) ?? []) {
      if (out.has(p) || p === id) continue;
      out.add(p);
      stack.push(p);
    }
  }
  return out;
}

const NORMAL: Record<Side, [number, number]> = { top: [0, -1], bottom: [0, 1], left: [-1, 0], right: [1, 0] };

function edgePath(s: Rect, t: Rect) {
  const a = edgeAnchors(s, t);
  const dist = Math.hypot(a.targetX - a.sourceX, a.targetY - a.sourceY);
  const k = Math.max(24, dist * 0.35);
  const [snx, sny] = NORMAL[a.sourceSide];
  const [tnx, tny] = NORMAL[a.targetSide];
  return `M${a.sourceX},${a.sourceY} C${a.sourceX + snx * k},${a.sourceY + sny * k} ${a.targetX + tnx * k},${a.targetY + tny * k} ${a.targetX},${a.targetY}`;
}

function el<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string | number> = {}) {
  const e = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  return e;
}

function wrap(text: string, width: number, lines: number): string[] {
  const out: string[] = [];
  let line = '';
  for (const word of text.split(' ')) {
    if ((line + ' ' + word).trim().length > width) {
      out.push(line.trim());
      line = '';
      if (out.length === lines) break;
    }
    line += ' ' + word;
  }
  if (out.length < lines && line.trim()) out.push(line.trim());
  return out;
}

// ---------- state ----------

type Style = (typeof LAYOUT_STYLES)[number];
type Pos = Map<string, { x: number; y: number }>;
interface Result {
  pos: Pos;
  ms: number;
  m: ReturnType<typeof metrics>;
}

const state = {
  graphs: [] as GalleryGraph[],
  current: 0,
  /** One layout shown large, or null to compare all. */
  focus: null as LayoutStyle | null,
  selected: null as string | null,
};

const cache = new Map<string, Result>();

function result(graph: GalleryGraph, style: LayoutStyle): Result {
  const key = `${state.current}:${style}`;
  let r = cache.get(key);
  if (!r) {
    const t0 = performance.now();
    const pos = layout(graph, style);
    r = { pos, ms: performance.now() - t0, m: metrics(graph, pos) };
    cache.set(key, r);
  }
  return r;
}

function saveHash() {
  const params = new URLSearchParams({ g: String(state.current) });
  if (state.focus) params.set('s', state.focus);
  history.replaceState(null, '', `#${params}`);
}

// ---------- rail ----------

function mergeCount(g: GalleryGraph) {
  const parents = new Map<string, number>();
  for (const e of g.edges) parents.set(e.target, (parents.get(e.target) ?? 0) + 1);
  return [...parents.values()].filter((n) => n > 1).length;
}

function renderRail() {
  const list = document.getElementById('graphs')!;
  list.replaceChildren(
    ...state.graphs.map((g, i) => {
      const li = document.createElement('li');
      const b = document.createElement('button');
      b.setAttribute('aria-current', String(i === state.current));
      const name = document.createElement('span');
      name.className = 'name';
      name.textContent = g.name;
      const facts = document.createElement('span');
      facts.className = 'facts';
      const merges = mergeCount(g);
      facts.textContent = `${g.nodes.length} nodes · ${g.edges.length} edges${merges ? ` · ${merges} merge${merges > 1 ? 's' : ''}` : ''}`;
      if (g.synthetic) {
        const tag = document.createElement('span');
        tag.className = 'tag';
        tag.textContent = ' · synthetic';
        facts.appendChild(tag);
      }
      b.append(name, facts);
      b.onclick = () => selectGraph(i);
      li.appendChild(b);
      return li;
    })
  );
}

// ---------- scoreboard ----------

function renderBoard(graph: GalleryGraph) {
  const rows = LAYOUT_STYLES.map((style) => ({ style, r: result(graph, style.id) }));
  const best = {
    fit: Math.max(...rows.map((x) => x.r.m.fitZoom)),
    crossings: Math.min(...rows.map((x) => x.r.m.crossings)),
    through: Math.min(...rows.map((x) => x.r.m.throughNodes)),
    edge: Math.min(...rows.map((x) => x.r.m.meanEdge)),
  };
  const mark = (isBest: boolean) => (isBest ? 'num best' : 'num');

  document.getElementById('board')!.replaceChildren(
    ...rows.map(({ style, r }, i) => {
      const tr = document.createElement('tr');
      tr.setAttribute('aria-selected', String(state.focus === style.id));
      const fitBest = r.m.fitZoom === best.fit;
      tr.innerHTML = `
        <td class="key"><kbd>${i + 1}</kbd></td>
        <td><span class="style">${glyphSvg(style.id, 18)}${style.label} <span class="hint">${style.hint}</span></span></td>
        <td><span class="fit${fitBest ? ' best' : ''}"><span class="track"><span class="bar" style="width:${(r.m.fitZoom / best.fit) * 100}%"></span></span><span class="value${fitBest ? ' best' : ''}">${Math.round(r.m.fitZoom * 100)}%</span></span></td>
        <td class="${mark(r.m.crossings === best.crossings)}">${r.m.crossings}</td>
        <td class="${mark(r.m.throughNodes === best.through)}">${r.m.throughNodes}</td>
        <td class="${mark(r.m.meanEdge === best.edge)}">${Math.round(r.m.meanEdge)} px</td>
        <td class="num">${r.ms < 1 ? '<1' : Math.round(r.ms)} ms</td>`;
      tr.onclick = () => setFocus(state.focus === style.id ? null : style.id);
      return tr;
    })
  );
}

// ---------- canvases ----------

interface Canvas {
  svg: SVGSVGElement;
  nodes: Map<string, SVGGElement>;
  edges: Array<{ el: SVGPathElement; source: string; target: string }>;
  trail: HTMLElement;
}

let canvases: Canvas[] = [];

function wrapLabel(text: string, chars: number, lines: number) {
  const out = wrap(text, chars, lines);
  if (out.length === lines && out.join(' ').length < text.length) out[lines - 1] = `${out[lines - 1].replace(/\s+\S*$/, '')}…`;
  return out;
}

function drawNode(n: GalleryNode, p: { x: number; y: number }) {
  const g = el('g', { class: `node ${n.role}`, transform: `translate(${p.x},${p.y})` });
  g.appendChild(el('rect', { width: W, height: H, rx: 6 }));
  if (n.role === 'file') {
    const tag = el('text', { x: 12, y: 22, class: 'file-tag' });
    tag.textContent = 'FILE';
    g.appendChild(tag);
  } else {
    g.appendChild(el('circle', { cx: 17, cy: 18, r: 4, class: 'dot' }));
    const role = el('text', { x: 28, y: 22, class: 'role' });
    role.textContent = n.role === 'user' ? 'User' : 'Agent';
    g.appendChild(role);
  }
  wrapLabel(n.label, 21, 4).forEach((line, i) => {
    const t = el('text', { x: 12, y: 46 + i * 18, class: 'text' });
    t.textContent = line;
    g.appendChild(t);
  });
  return g;
}

function drawCanvas(graph: GalleryGraph, pos: Pos): Omit<Canvas, 'trail'> {
  const xs = [...pos.values()].map((p) => p.x);
  const ys = [...pos.values()].map((p) => p.y);
  const pad = 60;
  const box = {
    x: Math.min(...xs) - pad,
    y: Math.min(...ys) - pad,
    w: Math.max(...xs) - Math.min(...xs) + W + 2 * pad,
    h: Math.max(...ys) - Math.min(...ys) + H + 2 * pad,
  };
  const svg = el('svg', { class: 'canvas', viewBox: `${box.x} ${box.y} ${box.w} ${box.h}` });
  svg.dataset.home = JSON.stringify(box);
  const rect = (id: string): Rect => ({ ...pos.get(id)!, width: W, height: H });

  const edgeLayer = el('g');
  const edges = graph.edges.map((e) => {
    const path = el('path', { d: edgePath(rect(e.source), rect(e.target)), class: 'edge' });
    edgeLayer.appendChild(path);
    return { el: path, source: e.source, target: e.target };
  });
  svg.appendChild(edgeLayer);

  const nodes = new Map<string, SVGGElement>();
  for (const n of graph.nodes) {
    const g = drawNode(n, pos.get(n.id)!);
    g.dataset.id = n.id;
    nodes.set(n.id, g);
    svg.appendChild(g);
  }
  return { svg, nodes, edges };
}

/** Restyle every canvas for the current selection, so colours can fade. */
function paintSelection() {
  const graph = state.graphs[state.current];
  const lineage = lineageOf(graph, state.selected);
  const label = graph.nodes.find((n) => n.id === state.selected)?.label;
  for (const c of canvases) {
    c.svg.classList.toggle('has-selection', state.selected !== null);
    for (const [id, g] of c.nodes) {
      g.classList.toggle('selected', id === state.selected);
      g.classList.toggle('lineage', lineage.has(id));
    }
    for (const e of c.edges) {
      e.el.classList.toggle('lineage', lineage.has(e.source) && (lineage.has(e.target) || e.target === state.selected));
    }
    c.trail.classList.toggle('shown', state.selected !== null);
    if (state.selected) {
      const count = lineage.size;
      c.trail.innerHTML = '';
      const strong = document.createElement('strong');
      strong.textContent = label || state.selected;
      c.trail.append(strong, ` · ${count === 0 ? 'a root, nothing leads here' : `${count} node${count > 1 ? 's' : ''} lead here`}`);
    }
  }
}

function attachInteractions(svg: SVGSVGElement, style: LayoutStyle) {
  let drag: { x: number; y: number; box: number[]; moved: boolean } | null = null;
  const box = () => svg.getAttribute('viewBox')!.split(' ').map(Number);
  const scale = () => box()[2] / svg.clientWidth;

  svg.addEventListener('click', (ev) => {
    if (drag?.moved) return;
    const node = (ev.target as Element).closest<SVGGElement>('.node');
    if (node) {
      state.selected = state.selected === node.dataset.id ? null : node.dataset.id!;
      paintSelection();
    } else if (!state.focus) {
      setFocus(style);
    } else {
      state.selected = null;
      paintSelection();
    }
  });

  if (state.focus !== style) return;

  svg.addEventListener(
    'wheel',
    (ev) => {
      ev.preventDefault();
      const [x, y, w, h] = box();
      const r = svg.getBoundingClientRect();
      // Keep the point under the cursor fixed while zooming.
      const k = Math.exp(ev.deltaY * 0.0015);
      const fx = (ev.clientX - r.left) / r.width;
      const fy = (ev.clientY - r.top) / r.height;
      const nw = w * k;
      const nh = h * k;
      svg.setAttribute('viewBox', `${x + (w - nw) * fx} ${y + (h - nh) * fy} ${nw} ${nh}`);
    },
    { passive: false }
  );
  svg.addEventListener('pointerdown', (ev) => {
    drag = { x: ev.clientX, y: ev.clientY, box: box(), moved: false };
    svg.setPointerCapture(ev.pointerId);
  });
  svg.addEventListener('pointermove', (ev) => {
    if (!drag || !svg.hasPointerCapture(ev.pointerId)) return;
    const dx = ev.clientX - drag.x;
    const dy = ev.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < 4) return;
    drag.moved = true;
    svg.classList.add('panning');
    const [x, y, w, h] = drag.box;
    const k = scale();
    svg.setAttribute('viewBox', `${x - dx * k} ${y - dy * k} ${w} ${h}`);
  });
  svg.addEventListener('pointerup', (ev) => {
    svg.releasePointerCapture(ev.pointerId);
    svg.classList.remove('panning');
    // Let the click handler see whether this was a drag, then forget it.
    setTimeout(() => (drag = null));
  });
}

function renderStage(graph: GalleryGraph) {
  const stage = document.getElementById('stage')!;
  stage.classList.toggle('focus', state.focus !== null);
  canvases = [];
  const styles = state.focus ? LAYOUT_STYLES.filter((s) => s.id === state.focus) : LAYOUT_STYLES;

  stage.replaceChildren(
    ...styles.map((style: Style) => {
      const r = result(graph, style.id);
      const cell = document.createElement('section');
      cell.className = 'cell';
      const header = document.createElement('header');
      header.innerHTML = `${glyphSvg(style.id, 16)}<span class="name">${style.label}</span><span class="meta">${Math.round(r.m.width)} × ${Math.round(r.m.height)} px · ${Math.round(r.m.fitZoom * 100)}%</span>`;
      if (state.focus) {
        const fit = document.createElement('button');
        fit.textContent = 'Fit';
        fit.title = 'Show the whole graph';
        fit.onclick = () => canvas.svg.setAttribute('viewBox', Object.values(JSON.parse(canvas.svg.dataset.home!) as object).join(' '));
        const back = document.createElement('button');
        back.textContent = 'Compare all';
        back.onclick = () => setFocus(null);
        header.append(fit, back);
      }
      const trail = document.createElement('div');
      trail.className = 'trail';
      const canvas = { ...drawCanvas(graph, r.pos), trail };
      canvas.svg.setAttribute('aria-label', `${style.label} layout of ${graph.name}`);
      attachInteractions(canvas.svg, style.id);
      canvases.push(canvas);
      cell.append(header, canvas.svg, trail);
      return cell;
    })
  );
  paintSelection();
}

// ---------- navigation ----------

function render() {
  const graph = state.graphs[state.current];
  renderRail();
  renderBoard(graph);
  renderStage(graph);
  saveHash();
}

function selectGraph(i: number) {
  state.current = (i + state.graphs.length) % state.graphs.length;
  state.selected = null;
  render();
}

function setFocus(style: LayoutStyle | null) {
  state.focus = style;
  render();
}

function onKey(ev: KeyboardEvent) {
  if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
  const n = Number(ev.key);
  if (ev.key >= '1' && ev.key <= String(LAYOUT_STYLES.length)) setFocus(LAYOUT_STYLES[n - 1].id);
  else if (ev.key === '0') setFocus(null);
  else if (ev.key === 'ArrowDown') selectGraph(state.current + 1);
  else if (ev.key === 'ArrowUp') selectGraph(state.current - 1);
  else if (ev.key === 'Escape' && state.selected) {
    state.selected = null;
    paintSelection();
  } else if (ev.key === 'Escape') setFocus(null);
  else return;
  ev.preventDefault();
}

async function loadLocalGraphs(): Promise<GalleryGraph[]> {
  try {
    const res = await fetch(new URL('./graphs.local', import.meta.url));
    if (!res.ok) return [];
    return (await res.json()) as GalleryGraph[];
  } catch {
    return [];
  }
}

async function main() {
  state.graphs = [researchDay(), deepThread(), ...(await loadLocalGraphs())];
  const params = new URLSearchParams(location.hash.slice(1));
  state.current = Math.min(state.graphs.length - 1, Math.max(0, Number(params.get('g') ?? 0) || 0));
  const s = params.get('s');
  state.focus = LAYOUT_STYLES.some((x) => x.id === s) ? (s as LayoutStyle) : null;
  document.addEventListener('keydown', onKey);
  render();
}

void main();
