import { describe, expect, it } from 'vitest';

import type { LayoutFixture } from './fixtures';
import { computeLayout } from './index';

// The shared parity fixture; crates/thoughttree-gpui-model must match it too.
const cases = import.meta.glob<LayoutFixture>('../../../fixtures/layout/*.json', {
  eager: true,
  import: 'default',
});

describe('layout parity fixture', () => {
  it('has cases', () => {
    expect(Object.keys(cases).length).toBeGreaterThanOrEqual(5);
  });

  it.each(Object.entries(cases))('%s', (_, fixture) => {
    const positions = computeLayout(fixture.nodes, fixture.edges, fixture.options);
    expect(Object.fromEntries(positions)).toEqual(fixture.expected);
  });
});
