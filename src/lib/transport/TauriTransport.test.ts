import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
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

  it('imports a Kagi export through the backend transport seam', async () => {
    const graphDto = {
      version: 4,
      nodes: [{ id: 'import:Example%20conversation:turn:0:user', role: 'user', content: 'Question', timestamp: 0 }],
      edges: [],
      layout: [{ id: 'import:Example%20conversation:turn:0:user', position: { x: 0, y: 0 } }],
    };
    vi.mocked(invoke).mockResolvedValue({ title: 'Example conversation', graph: graphDto });

    const transport = new TauriTransport();

    await expect(transport.importKagiExport('/tmp/export.json')).resolves.toEqual({
      title: 'Example conversation',
      graph: {
        nodes: new Map([[graphDto.nodes[0].id, graphDto.nodes[0]]]),
        edges: [],
        layout: new Map([[graphDto.layout[0].id, graphDto.layout[0].position]]),
      },
    });
    expect(invoke).toHaveBeenCalledWith('import_kagi_export', { path: '/tmp/export.json' });
  });
});
