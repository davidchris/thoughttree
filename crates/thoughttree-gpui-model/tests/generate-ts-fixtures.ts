// Run from the repository root with:
// bun crates/thoughttree-gpui-model/tests/generate-ts-fixtures.ts
// These golden cases are evaluated by the TypeScript GraphModel, the Project reader of Tauri v0.6.0.
// The palette cases came from the removed TypeScript palette. They are kept as they are; edit them by hand.
import { GraphModel } from '../../../packages/graph-model/src/model';
import { GraphSerialize } from '../../../packages/graph-model/src/serialize';
import { normalizeProvenance } from '../../../packages/graph-model/src/normalize';
import { parseKagiExport } from '../../../packages/graph-model/src/kagi';
import { conversationToGraph } from '../../../packages/graph-model/src/import';
import type { GraphNode, GraphEdge } from '../../../packages/graph-model/src/types';

const user = (id: string, content = id, timestamp = 1): GraphNode => ({ id, role: 'user', content, timestamp });
const assistant = (id: string, content = id, timestamp = 1): GraphNode => ({ id, role: 'assistant', content, timestamp });
const file = (id: string, path: string, timestamp = 1): GraphNode => ({ id, role: 'file', content: '', timestamp, path, name: path.split('/').at(-1)!, mimeType: 'text/markdown', size: 5, seenMtime: 10, seenSize: 5 });
const edge = (source: string, target: string): GraphEdge => ({ id: `${source}->${target}`, source, target });
const fixture = (name: string, nodes: GraphNode[], edges: GraphEdge[], target: string) => {
  const graph = { nodes: new Map(nodes.map(node => [node.id, node])), edges, layout: new Map(nodes.map(node => [node.id, { x: 0, y: 0 }])) };
  return { name, project: { version: 5, graph: GraphSerialize.toJSON(graph) }, target, pathIds: GraphModel.conversationPathIds(graph, target), messages: GraphModel.conversationPath(graph, target) };
};
const cases = [
  fixture('linear', [user('a'), assistant('b'), user('c')], [edge('a', 'b'), edge('b', 'c')], 'c'),
  fixture('unrelated siblings stay private', [user('a'), assistant('b'), assistant('secret'), user('c')], [edge('a', 'b'), edge('a', 'secret'), edge('b', 'c')], 'c'),
  fixture('diamond', [assistant('root', 'base', 1), user('left', 'branch one', 2), user('right', 'branch two', 2), assistant('done', 'answer', 3)], [edge('root', 'left'), edge('root', 'right'), edge('left', 'done'), edge('right', 'done')], 'done'),
  fixture('colliding short ids', [assistant('abcd-left', 'left', 1), assistant('abcd-right', 'right', 2), user('merge', 'synthesize', 3)], [edge('abcd-left', 'merge'), edge('abcd-right', 'merge')], 'merge'),
  fixture('blank intermediate node', [assistant('a', 'visible'), assistant('b', '   '), user('c', 'compare')], [edge('a', 'c'), edge('b', 'c')], 'c'),
  fixture('file lineage', [file('f1', 'notes/a.md'), file('f2', 'b.md'), user('u', 'compare')], [edge('f1', 'u'), edge('f2', 'u')], 'u'),
  fixture('file only input', [file('f', 'a.md'), user('u', '')], [edge('f', 'u')], 'u'),
  fixture('image-only merge', [{ ...user('image', ''), images: [{ data: 'cGl4ZWw=', mimeType: 'image/png', name: 'image.png' }] }, user('u', 'look')], [edge('image', 'u')], 'u'),
  fixture('legacy cycle tolerance', [user('a'), assistant('b'), user('c')], [edge('a', 'b'), edge('b', 'a'), edge('b', 'c')], 'c'),
];
// Deterministic timestamps and adjacency variation exercise stable ordering.
for (let seed = 1; seed <= 12; seed++) {
  const nodes = Array.from({ length: 8 }, (_, index) => (index % 2 ? assistant : user)(`n${index}`, index === seed % 8 ? '' : `text-${index}`, (index * seed) % 4));
  const edges: GraphEdge[] = [];
  for (let target = 1; target < 8; target++) {
    edges.push(edge(`n${target - 1}`, `n${target}`));
    if (target > 2 && (target + seed) % 2 === 0) edges.push(edge(`n${target - 3}`, `n${target}`));
  }
  cases.push(fixture(`timestamp dag ${seed}`, nodes, edges, 'n7'));
}
const provenance = [
  { completeness: 'complete', references: [{ type: 'url', url: 'https://example.com', title: 'Example', relations: ['cited'] }], activity: [{ type: 'commentary', content: 'Searching', timestamp: 10 }] },
  { completeness: 'complete', references: [{ type: 'file', scope: 'vault', path: '../secret.txt', displayName: '/Users/alice/secret.txt', relations: ['read', 'bogus'] }, { type: 'url', url: 'file:///Users/alice/key', relations: [] }], activity: [{ type: 'tool', kind: 'execute', title: 'echo hello', status: 'completed', rawInput: 'secret' }, { type: 'unknown', providerType: 'newType', label: 'Unsupported' }] },
  { completeness: 'unknown', references: [{ type: 'url', url: 'custom:thing', title: 'reading /Users/alice/doc', relations: ['consulted'] }], activity: [{ type: 'tool', kind: 'read', title: 'Read src/lib.rs', status: 'unexpected' }, { type: 'raw', payload: 'drop me' }] },
].map(input => ({ input, expected: normalizeProvenance(input) }));
const output = new URL('./fixtures/typescript-parity.json', import.meta.url);
const { palette } = await Bun.file(output).json();
const imports = [];
for (const name of ['kagi-export-v1.json', 'kagi-export-nested.json']) {
  const source = Bun.file(new URL(`../../../test/fixtures/${name}`, import.meta.url));
  if (await source.exists()) {
    const input = await source.text();
    imports.push({ name, input, graph: GraphSerialize.toJSON(conversationToGraph(parseKagiExport(input))) });
  }
}
await Bun.write(output, JSON.stringify({ cases, provenance, palette, imports }, null, 2) + '\n');
