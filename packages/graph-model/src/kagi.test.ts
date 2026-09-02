import { describe, expect, it } from 'vitest';
import kagiExportV1 from '../../../test/fixtures/kagi-export-v1.json';
import kagiExportV99 from '../../../test/fixtures/kagi-export-v99.json';
import {
  KAGI_EXPORT_MAX_BYTES,
  KagiExportError,
  parseKagiExport,
} from './kagi';

describe('parseKagiExport', () => {
  it('parses v1 into provider-neutral turns and preserves assistant bytes', () => {
    const conversation = parseKagiExport(JSON.stringify(kagiExportV1));

    expect(conversation).toEqual({
      importKey: 'Example research conversation',
      turns: [
        {
          userMessage: 'Compare the example sources.',
          assistantAnswer:
            'The fetched page supports the first point【1】, while the search result supports the second【2】. A dangling citation is retained【9】.',
          model: 'example-model',
          provenance: {
            completeness: 'complete',
            references: [
              {
                type: 'url',
                url: 'https://example.com/articles/fetched-page',
                title: 'Fetched example page',
                domain: 'example.com',
                index: 1,
                percentage: 96,
                is_search_result: false,
                relations: ['cited'],
              },
              {
                type: 'url',
                url: 'https://search.example.com/results/cited',
                title: 'Example search result',
                domain: 'search.example.com',
                index: 2,
                percentage: 88,
                is_search_result: true,
                relations: ['cited'],
              },
              {
                type: 'url',
                url: 'https://example.com/results/consulted-only',
                title: 'Consulted search result',
                domain: 'example.com',
                index: 3,
                percentage: 71,
                is_search_result: true,
                relations: ['consulted'],
              },
            ],
            activity: [],
          },
        },
        {
          userMessage: 'Give a brief conclusion.',
          assistantAnswer: 'The example sources illustrate the requested reference shapes.',
          model: 'example-model',
          provenance: {
            completeness: 'complete',
            references: [],
            activity: [],
          },
        },
      ],
    });
  });

  it('rejects unsupported versions with the version found', () => {
    expect(() => parseKagiExport(JSON.stringify(kagiExportV99))).toThrowError(
      new KagiExportError('unsupported_version', 99)
    );
  });

  it('rejects empty or missing messages', () => {
    expect(() => parseKagiExport(JSON.stringify({ version: 1, messages: [] }))).toThrowError(
      new KagiExportError('no_messages')
    );

    expect(() => parseKagiExport(JSON.stringify({ version: 1 }))).toThrowError(
      new KagiExportError('no_messages')
    );

    expect(
      parseKagiExport(JSON.stringify({
        version: 1,
        conversation: { messages: [{ role: 'user', content: 'Question' }, { role: 'assistant', content: 'Answer' }] },
      })).turns[0].provenance
    ).toEqual({ completeness: 'complete', references: [], activity: [] });
  });

  it('pairs only adjacent user/assistant messages and marks a trailing user incomplete', () => {
    const conversation = parseKagiExport(JSON.stringify({
      version: 1,
      conversation: {
        title: 'Roles',
        messages: [
          { role: 'assistant', content: 'orphan' },
          { role: 'user', content: 'first' },
          { role: 'user', content: 'second' },
          { role: 'assistant', content: 'answer' },
          { role: 'assistant', content: 'orphan two' },
          { role: 'user', content: 'trailing' },
        ],
      },
    }));

    expect(conversation.turns).toEqual([
      {
        userMessage: 'first',
        assistantAnswer: '',
        incomplete: true,
      },
      {
        userMessage: 'second',
        assistantAnswer: 'answer',
        provenance: { completeness: 'complete', references: [], activity: [] },
      },
      { userMessage: 'trailing', assistantAnswer: '', incomplete: true },
    ]);
  });

  it('enforces the input size cap before parsing', () => {
    const oversized = JSON.stringify({ version: 1, padding: 'x'.repeat(KAGI_EXPORT_MAX_BYTES) });

    expect(() => parseKagiExport(oversized)).toThrowError(KagiExportError);
    try {
      parseKagiExport(oversized);
    } catch (error) {
      expect(error).toMatchObject({
        code: 'input_too_large',
        inputBytes: expect.any(Number),
      });
    }
  });
});
