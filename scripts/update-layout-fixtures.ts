// Rewrites `expected` in every fixtures/layout/*.json case from the TS Tidy.
// Run after an intended layout change: bun scripts/update-layout-fixtures.ts
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { computeLayout } from '../src/lib/layout';
import type { LayoutFixture } from '../src/lib/layout/fixtures';

const dir = join(import.meta.dir, '..', 'fixtures', 'layout');
for (const file of readdirSync(dir).filter((f) => f.endsWith('.json'))) {
  const path = join(dir, file);
  const fixture = JSON.parse(readFileSync(path, 'utf8')) as LayoutFixture;
  const positions = computeLayout(fixture.nodes, fixture.edges, fixture.options);
  fixture.expected = Object.fromEntries(fixture.nodes.map((n) => [n.id, positions.get(n.id)!]));
  writeFileSync(path, `${JSON.stringify(fixture, null, 2)}\n`);
}
