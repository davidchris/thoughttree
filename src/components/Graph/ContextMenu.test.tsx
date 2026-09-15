import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { BackendTransport } from '../../lib/transport';
import { setBackendTransport } from '../../lib/transport';
import { createMockTransport } from '../../test/mockTransport';
import { useGraphStore } from '../../store/useGraphStore';
import { useUIStore } from '../../store/useUIStore';
import { ContextMenu } from './ContextMenu';

const NOTE = {
  path: 'notes/plan.md',
  name: 'plan.md',
  mimeType: 'text/markdown',
  size: 2048,
  seenMtime: 1_000,
  seenSize: 2048,
};

describe('ContextMenu on a file node', () => {
  let transport: BackendTransport;

  beforeEach(() => {
    transport = createMockTransport();
    setBackendTransport(transport);
    useGraphStore.getState().newProject();
    useUIStore.getState().reset();
  });

  it('"Reload file" adopts the current on-disk version, the same as the card button', async () => {
    vi.mocked(transport.statVaultFile).mockResolvedValue({
      status: 'ok',
      stat: { size: 4096, modifiedEpochMs: 2_000 },
      mimeType: 'text/markdown',
      name: 'plan.md',
    });
    const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });
    await useGraphStore.getState().refreshFileNodeStat(id);
    expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('changed');
    const onClose = vi.fn();

    render(<ContextMenu x={0} y={0} target={{ kind: 'node', nodeId: id }} onClose={onClose} />);
    await userEvent.click(screen.getByRole('button', { name: 'Reload file' }));

    await waitFor(() =>
      expect(useGraphStore.getState().nodeData.get(id)).toMatchObject({ seenMtime: 2_000, seenSize: 4096 })
    );
    expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('ok');
    expect(onClose).toHaveBeenCalled();
  });
});
