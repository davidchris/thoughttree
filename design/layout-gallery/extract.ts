/**
 * Reduce .thoughttree files to what the layout gallery draws: ids, roles,
 * one-line labels, positions and edges. Output stays local (graphs.local is
 * gitignored) because it carries conversation summaries.
 *
 *   bun design/layout-gallery/extract.ts ~/Notes/*.thoughttree
 */
import { basename } from 'node:path';

interface GalleryNode {
  id: string;
  role: string;
  label: string;
  x: number;
  y: number;
}

interface RawNode {
  id: string;
  role?: string;
  summary?: string;
  content?: string;
  name?: string;
  path?: string;
}

function label(n: RawNode): string {
  const text = n.summary || n.name || n.path || n.content || '';
  return text.replace(/\s+/g, ' ').trim().slice(0, 70);
}

function extract(raw: Record<string, unknown>) {
  const nested = raw.graph as Record<string, unknown> | undefined;
  if (nested) {
    const layout = new Map(
      (nested.layout as Array<{ id: string; position: { x: number; y: number } }>).map((l) => [l.id, l.position])
    );
    const nodes = (nested.nodes as RawNode[]).map((n) => ({
      id: n.id,
      role: n.role ?? 'user',
      label: label(n),
      ...(layout.get(n.id) ?? { x: 0, y: 0 }),
    }));
    return { nodes, edges: nested.edges as Array<{ source: string; target: string }> };
  }
  // Version 1: ReactFlow nodes with the domain node under data.nodeData.
  const flow = raw.nodes as Array<{ id: string; position: { x: number; y: number }; data: { nodeData: RawNode } }>;
  const nodes: GalleryNode[] = flow.map((n) => ({
    id: n.id,
    role: n.data.nodeData.role ?? 'user',
    label: label(n.data.nodeData),
    ...n.position,
  }));
  return { nodes, edges: raw.edges as Array<{ source: string; target: string }> };
}

const graphs = [];
for (const file of process.argv.slice(2)) {
  const { nodes, edges } = extract(JSON.parse(await Bun.file(file).text()) as Record<string, unknown>);
  if (nodes.length < 3) continue;
  graphs.push({
    name: basename(file, '.thoughttree'),
    nodes,
    edges: edges.map(({ source, target }) => ({ source, target })),
  });
}
graphs.sort((a, b) => b.nodes.length - a.nodes.length);
await Bun.write(new URL('./graphs.local', import.meta.url), JSON.stringify(graphs));
console.log(graphs.map((g) => `${g.name}: ${g.nodes.length} nodes, ${g.edges.length} edges`).join('\n'));
