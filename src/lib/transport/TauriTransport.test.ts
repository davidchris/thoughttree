import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import kagiExport from '../../../test/fixtures/kagi-export-v1.json';
import { TauriTransport } from './TauriTransport';
import { KagiImportError } from './types';

describe('TauriTransport', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('lists projects via the dedicated metadata command', async () => {
    const entries = [{ relativePath: 'alpha.thoughttree', modifiedEpochMs: 1720000000000 }];
    vi.mocked(invoke).mockResolvedValue(entries);

    const transport = new TauriTransport();

    await expect(transport.listProjects()).resolves.toEqual(entries);
    expect(invoke).toHaveBeenCalledWith('list_projects');
  });

  it('preserves Turn identity on stream events', async () => {
    const transport = new TauriTransport();
    const receive = vi.fn();
    const unsubscribe = transport.onStreamChunk(receive);
    await Promise.resolve();
    const listener = vi.mocked(listen).mock.calls.find(([name]) => name === 'stream-chunk')![1];
    const event = { event: 'stream-chunk', id: 1, payload: { node_id: 'node', turn_id: 'turn', chunk: 'answer' } };

    listener(event);
    expect(receive).toHaveBeenCalledWith({ nodeId: 'node', turnId: 'turn', chunk: 'answer' });
    unsubscribe();
    listener(event);
    expect(receive).toHaveBeenCalledTimes(1);
  });

  it('preserves Turn identity on provenance events', async () => {
    const transport = new TauriTransport();
    const receive = vi.fn();
    transport.onTurnProvenance(receive);
    await Promise.resolve();
    const listener = vi.mocked(listen).mock.calls.find(([name]) => name === 'turn-provenance')![1];

    listener({
      event: 'turn-provenance',
      id: 1,
      payload: { node_id: 'node', turn_id: 'turn', provenance: { completeness: 'complete' } },
    });

    expect(receive).toHaveBeenCalledWith({
      nodeId: 'node',
      turnId: 'turn',
      provenance: { completeness: 'complete' },
    });
  });

  it('waits for stream listeners before starting a Turn', async () => {
    let ready!: (unlisten: () => void) => void;
    vi.mocked(listen).mockImplementationOnce(() => new Promise((resolve) => { ready = resolve; }));
    vi.mocked(invoke).mockResolvedValue('ok');
    const transport = new TauriTransport();
    const sending = transport.sendPrompt({ nodeId: 'node', turnId: 'turn', messages: [{ role: 'user', content: 'Hello' }] });
    await Promise.resolve();
    expect(invoke).not.toHaveBeenCalled();

    ready(() => {});
    await expect(sending).resolves.toBe('ok');
    expect(invoke).toHaveBeenCalledWith('send_prompt', {
      request: expect.objectContaining({ nodeId: 'node', turnId: 'turn' }),
    });
  });

  it('reads and imports a Kagi export through the frontend graph-model seam', async () => {
    vi.mocked(invoke).mockResolvedValue(JSON.stringify(kagiExport));

    const transport = new TauriTransport();
    const imported = await transport.importKagiExport('/tmp/export.json');

    expect(imported.title).toBe('Example research conversation');
    expect(imported.graph.nodes).toHaveLength(4);
    expect(imported.graph.edges).toHaveLength(3);
    expect(imported.graph.nodes.get('import:Example%20research%20conversation:turn:0:assistant')).toMatchObject({
      id: 'import:Example%20research%20conversation:turn:0:assistant',
      content: 'The fetched page supports the first point【1】, while the search result supports the second【2】. A dangling citation is retained【9】.',
      model: 'example-model',
      provenance: {
        completeness: 'complete',
        references: [
          expect.objectContaining({ index: 1, relations: ['cited'] }),
          expect.objectContaining({ index: 2, relations: ['cited'] }),
          expect.objectContaining({ index: 3, relations: ['consulted'] }),
        ],
      },
    });
    expect(invoke).toHaveBeenCalledWith('import_kagi_export', { path: '/tmp/export.json' });
  });

  it('maps typed backend Kagi import failures to KagiImportError', async () => {
    vi.mocked(invoke).mockRejectedValue({
      kind: 'input_too_large',
      message: 'Kagi export exceeds the 16777216-byte input limit (16777217 bytes)',
      input_bytes: 16777217,
      limit_bytes: 16777216,
    });

    const transport = new TauriTransport();
    const failure = transport.importKagiExport('/tmp/huge.json');

    await expect(failure).rejects.toBeInstanceOf(KagiImportError);
    await expect(failure).rejects.toMatchObject({
      kind: 'input_too_large',
      message: 'Kagi export exceeds the 16777216-byte input limit (16777217 bytes)',
    });
  });

  it('sends file refs on prompt messages in the backend wire shape and keeps text-less file turns', async () => {
    vi.mocked(invoke).mockResolvedValue('ok');
    const transport = new TauriTransport();

    await transport.sendPrompt({
      nodeId: 'n',
      turnId: 'turn-1',
      messages: [
        { role: 'user', content: '', files: [{ path: 'notes/a.md', name: 'a.md', mimeType: 'text/markdown', size: 12 }] },
        { role: 'assistant', content: 'read it' },
        { role: 'user', content: '   ' },
      ],
    });

    expect(invoke).toHaveBeenCalledWith('send_prompt', {
      request: expect.objectContaining({
        nodeId: 'n',
        turnId: 'turn-1',
        messages: [
          {
            role: 'user',
            content: '',
            images: null,
            files: [{ path: 'notes/a.md', name: 'a.md', mime_type: 'text/markdown', size: 12 }],
          },
          { role: 'assistant', content: 'read it', images: null, files: null },
        ],
      }),
    });
  });

  it('maps vault file stat, preview and limits payloads to camelCase', async () => {
    const transport = new TauriTransport();

    vi.mocked(invoke).mockResolvedValueOnce({
      status: 'ok',
      stat: { size: 5, modified_epoch_ms: 1720000000000 },
      mime_type: 'text/markdown',
      name: 'note.md',
    });
    await expect(transport.statVaultFile('notes/note.md')).resolves.toEqual({
      status: 'ok',
      stat: { size: 5, modifiedEpochMs: 1720000000000 },
      mimeType: 'text/markdown',
      name: 'note.md',
    });
    expect(invoke).toHaveBeenCalledWith('stat_vault_file', { path: 'notes/note.md' });

    vi.mocked(invoke).mockResolvedValueOnce({ status: 'missing' });
    await expect(transport.statVaultFile('gone.md')).resolves.toEqual({ status: 'missing' });

    vi.mocked(invoke).mockResolvedValueOnce({
      info: { name: 'pic.png', mime_type: 'image/png', size: 900, modified_epoch_ms: 42 },
      preview: { kind: 'image', data: 'AAAA', mime_type: 'image/png', width: 512, height: 128 },
    });
    await expect(transport.readVaultFilePreview('pic.png')).resolves.toEqual({
      info: { name: 'pic.png', mimeType: 'image/png', size: 900, modifiedEpochMs: 42 },
      preview: { kind: 'image', data: 'AAAA', mimeType: 'image/png', width: 512, height: 128 },
    });

    vi.mocked(invoke).mockResolvedValueOnce({
      image_max_bytes: 5242880,
      image_max_side: 8000,
      max_images_per_prompt: 20,
      preview_text_bytes: 16384,
    });
    await expect(transport.getAttachmentLimits()).resolves.toEqual({
      imageMaxBytes: 5242880,
      imageMaxSide: 8000,
      maxImagesPerPrompt: 20,
      previewTextBytes: 16384,
    });
  });

  it('returns null when the vault file picker is cancelled', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(null);

    await expect(new TauriTransport().pickVaultFile()).resolves.toBeNull();
    expect(invoke).toHaveBeenCalledWith('pick_vault_file');
  });

  it('rethrows untyped backend failures unchanged', async () => {
    vi.mocked(invoke).mockRejectedValue('boom');

    const transport = new TauriTransport();

    await expect(transport.importKagiExport('/tmp/x.json')).rejects.toBe('boom');
  });
});
