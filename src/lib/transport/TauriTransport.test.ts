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
});
