import { describe, expect, it, vi } from 'vitest';
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

  it('keeps only valid http and https references', () => {
    const conversation = parseKagiExport(JSON.stringify({
      version: 1,
      conversation: {
        messages: [
          { role: 'user', content: 'Find sources.' },
          {
            role: 'assistant',
            content: 'Web source【1】 and unsafe source【2】.',
            references: [
              { url: 'https://example.com/safe', title: 'Safe source', index: 1 },
              { url: 'http://example.com/also-safe', title: 'Also safe', index: 5 },
              { url: 'file:///Users/alice/private.txt', title: 'Local evidence', index: 2 },
              { url: 'javascript:alert(1)', title: 'Script', index: 3 },
              { url: 'not a URL', title: 'Malformed', index: 4 },
            ],
          },
        ],
      },
    }));

    expect(conversation.turns[0].provenance?.references).toEqual([
      {
        type: 'url',
        url: 'https://example.com/safe',
        title: 'Safe source',
        index: 1,
        relations: ['cited'],
      },
      {
        type: 'url',
        url: 'http://example.com/also-safe',
        title: 'Also safe',
        index: 5,
        relations: ['consulted'],
      },
    ]);
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

  it('reports the effective byte limit when a custom cap rejects input', () => {
    expect(() => parseKagiExport('{"version":1}', 1)).toThrowError(
      'Kagi export exceeds the 1-byte input limit (13 bytes)'
    );
    try {
      parseKagiExport('{"version":1}', 1);
    } catch (error) {
      expect(error).toMatchObject({ code: 'input_too_large', inputBytes: 13, limitBytes: 1 });
    }
  });

  it('rejects oversized strings before encoding them', () => {
    const encode = vi.spyOn(TextEncoder.prototype, 'encode');
    try {
      expect(() => parseKagiExport('x'.repeat(9), 8)).toThrowError(KagiExportError);
      expect(encode).not.toHaveBeenCalled();
    } finally {
      encode.mockRestore();
    }
  });

  it('rejects oversized byte input before decoding it', () => {
    const decode = vi.spyOn(TextDecoder.prototype, 'decode');
    try {
      expect(() => parseKagiExport(new Uint8Array(9), 8)).toThrowError(KagiExportError);
      expect(decode).not.toHaveBeenCalled();
    } finally {
      decode.mockRestore();
    }
  });

  it('rejects malformed UTF-8 bytes instead of substituting U+FFFD', () => {
    const valid = new TextEncoder().encode(
      JSON.stringify({ version: 1, messages: [{ role: 'user', content: 'q' }, { role: 'assistant', content: 'a' }] })
    );
    const malformed = new Uint8Array(valid);
    malformed[malformed.lastIndexOf(0x61)] = 0x80; // replace the "a" answer byte with a stray continuation byte

    expect(() => parseKagiExport(malformed)).toThrowError(new KagiExportError('invalid_json'));
    expect(parseKagiExport(valid).turns[0].assistantAnswer).toBe('a');
  });

  it('classifies an uncited fetched page as consulted independent of source kind', () => {
    const conversation = parseKagiExport(JSON.stringify({
      version: 1,
      messages: [
        { role: 'user', content: 'Question' },
        {
          role: 'assistant',
          content: 'Only the search result is cited【2】.',
          references: [
            { url: 'https://example.com/fetched', index: 1, is_search_result: false },
            { url: 'https://example.com/search', index: 2, is_search_result: true },
          ],
        },
      ],
    }));

    expect(conversation.turns[0].provenance?.references).toEqual([
      { type: 'url', url: 'https://example.com/fetched', index: 1, is_search_result: false, relations: ['consulted'] },
      { type: 'url', url: 'https://example.com/search', index: 2, is_search_result: true, relations: ['cited'] },
    ]);
  });
});
