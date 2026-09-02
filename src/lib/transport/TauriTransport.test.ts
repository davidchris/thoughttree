import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import kagiExport from '../../../test/fixtures/kagi-export-v1.json';
import { TauriTransport } from './TauriTransport';

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
});
