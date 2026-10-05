// Run from the repository root with:
// bun crates/thoughttree-gpui-model/tests/generate-ts-fixtures.ts
// These golden cases are evaluated by the existing frontend's GraphModel.
import { GraphModel } from '../../../packages/graph-model/src/model';
import { GraphSerialize } from '../../../packages/graph-model/src/serialize';
import { normalizeProvenance } from '../../../packages/graph-model/src/normalize';
import { PaletteSearch } from '../../../src/lib/palette/search';
import type { HighlightedText } from '../../../src/lib/palette/types';
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
const byteSpans = (text: HighlightedText | undefined) => text ? ({
  text: text.text,
  spans: text.spans.map(span => ({
    start: new TextEncoder().encode(text.text.slice(0, span.start)).length,
    end: new TextEncoder().encode(text.text.slice(0, span.end)).length,
  })),
}) : null;
const paletteFixture = (name: string, corpus: GraphNode[], query: string) => ({
  name, corpus, query,
  expected: PaletteSearch.search(corpus, query, 20).hits.map(hit => ({ id: hit.node.id, title: byteSpans(hit.title), snippet: byteSpans(hit.snippet) })),
});
const palette = [
  paletteFixture('UTF16 match position ranking', [user('ascii', 'xxx rust'), user('unicode', 'é😀rust')], 'rust'),
  paletteFixture('title boundary excludes split surrogate', [user('title', 'x'.repeat(79) + '😀 suffix')], ''),
  paletteFixture('file title limit', [file('f', `notes/${'x'.repeat(79)}😀.md`)], ''),
  paletteFixture('folder query searches complete Vault path', [file('f', '研究/reference.md'), user('u', 'unrelated')], '研究'),
  paletteFixture('snippet window and whitespace preserve boundary space', [user('snippet', 'a'.repeat(80) + ' 😀\n  target\t  ' + 'b'.repeat(140))], 'target'),
  paletteFixture('Unicode regexp folding stays browser compatible', [user('ascii', 'k s'), user('kelvin', 'K'), user('long-s', 'ſ'), user('turkish', 'İ ı')], 'k'),
  paletteFixture('Unicode token matches have valid byte spans', [user('greek', 'σΣς'), user('turkish', 'İ ı')], 'σ'),
  paletteFixture('astral case folding stays browser compatible', [user('deseret', '𐐀 𐐨')], '𐐨'),
  paletteFixture('Javascript whitespace token boundaries', [user('and', 'rust parser'), user('one', 'rust')], 'rust\ufeffparser'),
  paletteFixture('overlapping literals merge highlights', [user('overlap', 'a+b aba ababa')], 'aba ba'),
  paletteFixture('summary ranks before early content', [user('body', 'Rust'), { ...assistant('summary', 'Later Rust'), summary: 'Rust overview' }], 'Rust'),
];
const authored = await Bun.file(new URL('../../../docs/gpui/fixtures/parity.thoughttree', import.meta.url)).json();
palette.push(paletteFixture('authored visual fixture Native rich answer', authored.graph.nodes, 'Native rich answer'));
const imports = [];
for (const name of ['kagi-export-v1.json', 'kagi-export-nested.json']) {
  const source = Bun.file(new URL(`../../../test/fixtures/${name}`, import.meta.url));
  if (await source.exists()) {
    const input = await source.text();
    imports.push({ name, input, graph: GraphSerialize.toJSON(conversationToGraph(parseKagiExport(input))) });
  }
}
await Bun.write(new URL('./fixtures/typescript-parity.json', import.meta.url), JSON.stringify({ cases, provenance, palette, imports }, null, 2) + '\n');
