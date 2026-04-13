import { describe, expect, it } from 'vitest';
import { hasFreshSummary } from './useSummaryGeneration';

type SummaryCheckInput = {
  timestamp: number;
  contentUpdatedAt?: number;
  summary?: string;
  summaryTimestamp?: number;
};

describe('useSummaryGeneration helpers', () => {
  it('treats summaries as stale after contentUpdatedAt moves past summaryTimestamp', () => {
    const data: SummaryCheckInput = {
      timestamp: 100,
      contentUpdatedAt: 250,
      summary: 'Older summary',
      summaryTimestamp: 200,
    };
    expect(
      hasFreshSummary(data)
    ).toBe(false);
  });

  it('treats summaries as fresh when summaryTimestamp is at least the content version', () => {
    const data: SummaryCheckInput = {
      timestamp: 100,
      contentUpdatedAt: 250,
      summary: 'Fresh summary',
      summaryTimestamp: 250,
    };
    expect(
      hasFreshSummary(data)
    ).toBe(true);
  });

  it('falls back to timestamp when contentUpdatedAt is missing', () => {
    const data: SummaryCheckInput = {
      timestamp: 100,
      summary: 'Fresh summary',
      summaryTimestamp: 101,
    };
    expect(
      hasFreshSummary(data)
    ).toBe(true);
  });
});
