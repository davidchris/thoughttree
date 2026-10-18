/** A node as the layouts see it: current top-left corner and rendered size. */
export interface LayoutNode {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface LayoutEdge {
  source: string;
  target: string;
}

/**
 * The graph reduced to what layouts need. Edges that would close a cycle are
 * dropped, so `parents` / `children` always describe a DAG.
 */
export interface Structure {
  nodes: Map<string, LayoutNode>;
  /** Every parent, in edge order. */
  parents: Map<string, string[]>;
  /** Every child, ordered by current position (x, then y) for stable layouts. */
  children: Map<string, string[]>;
  /** Weakly connected components; trees joined by a merge share one. */
  components: string[][];
}

function byPosition(nodes: Map<string, LayoutNode>) {
  return (a: string, b: string) => {
    const na = nodes.get(a)!;
    const nb = nodes.get(b)!;
    return na.x - nb.x || na.y - nb.y;
  };
}

function byReadingOrder(nodes: Map<string, LayoutNode>) {
  return (a: string, b: string) => {
    const na = nodes.get(a)!;
    const nb = nodes.get(b)!;
    return na.y - nb.y || na.x - nb.x;
  };
}

function push<T>(map: Map<string, T[]>, key: string, value: T) {
  const list = map.get(key);
  if (list) list.push(value);
  else map.set(key, [value]);
}

/** Union-find over node ids, used to group trees that a merge edge links. */
function findComponents(ids: string[], edges: LayoutEdge[]): Map<string, string> {
  const root = new Map(ids.map((id) => [id, id] as const));
  const find = (id: string): string => {
    let r = id;
    while (root.get(r) !== r) r = root.get(r)!;
    root.set(id, r);
    return r;
  };
  for (const e of edges) root.set(find(e.source), find(e.target));
  return new Map(ids.map((id) => [id, find(id)] as const));
}

/** Drop back edges found by a DFS that starts from sources, so the rest is acyclic. */
function acyclicEdges(ordered: string[], edges: LayoutEdge[]): LayoutEdge[] {
  const out = new Map<string, LayoutEdge[]>();
  const hasParent = new Set<string>();
  for (const e of edges) {
    push(out, e.source, e);
    hasParent.add(e.target);
  }

  const state = new Map<string, 'open' | 'done'>();
  const kept: LayoutEdge[] = [];
  const visit = (start: string) => {
    if (state.has(start)) return;
    const stack: Array<{ id: string; next: number }> = [{ id: start, next: 0 }];
    state.set(start, 'open');
    while (stack.length) {
      const frame = stack[stack.length - 1];
      const outgoing = out.get(frame.id) ?? [];
      if (frame.next >= outgoing.length) {
        state.set(frame.id, 'done');
        stack.pop();
        continue;
      }
      const e = outgoing[frame.next++];
      const seen = state.get(e.target);
      if (seen === 'open') continue; // back edge
      kept.push(e);
      if (!seen) {
        state.set(e.target, 'open');
        stack.push({ id: e.target, next: 0 });
      }
    }
  };

  for (const id of ordered) if (!hasParent.has(id)) visit(id);
  for (const id of ordered) visit(id); // nodes only reachable through a cycle
  return kept;
}

export function buildStructure(nodeList: LayoutNode[], edgeList: LayoutEdge[]): Structure {
  const nodes = new Map(nodeList.map((n) => [n.id, n] as const));
  const ordered = nodeList.map((n) => n.id).sort(byReadingOrder(nodes));

  const seen = new Set<string>();
  const valid = edgeList.filter((e) => {
    const key = `${e.source}\u0000${e.target}`;
    if (e.source === e.target || seen.has(key)) return false;
    if (!nodes.has(e.source) || !nodes.has(e.target)) return false;
    seen.add(key);
    return true;
  });
  const edges = acyclicEdges(ordered, valid);

  const parents = new Map<string, string[]>();
  const children = new Map<string, string[]>();
  for (const e of edges) {
    push(parents, e.target, e.source);
    push(children, e.source, e.target);
  }
  for (const kids of children.values()) kids.sort(byPosition(nodes));

  const componentOf = findComponents(ordered, valid);
  const grouped = new Map<string, string[]>();
  for (const id of ordered) push(grouped, componentOf.get(id)!, id);

  return { nodes, parents, children, components: [...grouped.values()] };
}

/** A tree over node ids, e.g. a breadth-first tree of one component. */
export interface SpanningTree {
  root: string;
  children: Map<string, string[]>;
  parent: Map<string, string>;
}

/** Number of leaves under each tree node, the usual weight for sharing space. */
export function leafCounts(tree: SpanningTree): Map<string, number> {
  const leaves = new Map<string, number>();
  for (const id of postOrder(tree)) {
    const kids = tree.children.get(id) ?? [];
    leaves.set(id, kids.length ? kids.reduce((sum, k) => sum + leaves.get(k)!, 0) : 1);
  }
  return leaves;
}

/** Children before parents, without recursion (conversations can be very deep). */
export function postOrder(tree: SpanningTree): string[] {
  const order: string[] = [];
  const stack = [tree.root];
  while (stack.length) {
    const id = stack.pop()!;
    order.push(id);
    for (const kid of tree.children.get(id) ?? []) stack.push(kid);
  }
  return order.reverse();
}
