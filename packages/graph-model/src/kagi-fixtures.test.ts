import { describe, expect, it } from 'vitest';
import kagiExportV1 from '../../../test/fixtures/kagi-export-v1.json';
import kagiExportV99 from '../../../test/fixtures/kagi-export-v99.json';

describe('Kagi export fixtures', () => {
  it('covers the supported parser and import shapes without personal data', () => {
    const fixture = kagiExportV1 as {
      version: number;
      exported_at: string;
      fixture_note: string;
      conversation: {
        title: string;
        model_name: string;
      };
      messages: Array<Record<string, unknown>>;
    };

    expect(fixture).toMatchObject({
      version: 1,
      exported_at: '2026-01-15T12:00:00Z',
      fixture_note: 'unknown top-level field',
      conversation: {
        title: 'Example research conversation',
        model_name: 'example-model',
      },
    });
    expect(fixture.messages).toHaveLength(4);
    expect(Buffer.byteLength(JSON.stringify(fixture))).toBeLessThan(5 * 1024);

    const [firstUser, firstAssistant, secondUser, secondAssistant] = fixture.messages;
    expect(firstUser).toMatchObject({ role: 'user' });
    expect(firstAssistant).toMatchObject({
      role: 'assistant',
      model_name: 'example-model',
      tokens: 42,
      cost_usd: 0.001,
      duration_ms: 250,
      model_version: 'example-model-2026-01-01',
      unknown_message_field: 'ignored',
    });
    expect(secondUser).toMatchObject({ role: 'user' });
    expect(secondAssistant).toMatchObject({ role: 'assistant' });
    expect(secondAssistant).not.toHaveProperty('references');

    const content = firstAssistant.content as string;
    expect(content).toContain('【1】');
    expect(content).toContain('【2】');
    expect(content).toContain('【9】');
    expect(content).not.toContain('【3】');

    const references = firstAssistant.references as Array<Record<string, unknown>>;
    expect(references).toHaveLength(3);
    expect(references[0]).toMatchObject({ index: 1, is_search_result: false, passages: null });
    expect(references[1]).toMatchObject({ index: 2, is_search_result: true });
    expect(references[1].passages).toEqual([
      { text: 'A short example passage.', start: 0, end: 24 },
    ]);
    expect(references[2]).toMatchObject({ index: 3, is_search_result: true });

    const urls = JSON.stringify(fixture).match(/https?:\/\/[^"\\]+/g)?.map((url) => new URL(url)) ?? [];
    expect(urls).not.toHaveLength(0);
    expect(urls.every((url) => url.hostname === 'example.com' || url.hostname.endsWith('.example.com')))
      .toBe(true);
  });

  it('provides an unsupported-version rejection case', () => {
    expect(kagiExportV99).toEqual({ version: 99 });
  });
});
