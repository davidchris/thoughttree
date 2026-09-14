import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import type { NodeProps } from '@xyflow/react';
import type { BackendTransport } from '../../lib/transport';
import { setBackendTransport } from '../../lib/transport';
import { createMockTransport } from '../../test/mockTransport';
import { useGraphStore } from '../../store/useGraphStore';
import { useUIStore } from '../../store/useUIStore';
import { UserNode } from './UserNode';

const NOTE = {
  path: 'notes/plan.md',
  name: 'plan.md',
  mimeType: 'text/markdown',
  size: 120,
  seenMtime: 1_000,
  seenSize: 120,
};

function renderUserNode(id: string) {
  const props = { id, selected: false } as unknown as NodeProps;
  return render(
    <ReactFlowProvider>
      <UserNode {...props} />
    </ReactFlowProvider>
  );
}

describe('UserNode Generate button', () => {
  let transport: BackendTransport;

  beforeEach(() => {
    transport = createMockTransport();
    setBackendTransport(transport);
    useGraphStore.getState().newProject();
    useUIStore.getState().reset();
  });

  it('is disabled and explains why when a lineage file node is missing', async () => {
    vi.mocked(transport.statVaultFile).mockResolvedValue({ status: 'missing' });
    const state = useGraphStore.getState();
    const fileId = state.addFileNode(NOTE, { x: 0, y: 0 });
    const userId = state.createUserNodeDownstream(fileId);
    state.updateNodeContent(userId, 'Summarise the plan');
    await useGraphStore.getState().refreshFileNodeStat(fileId);
    useUIStore.getState().setEditing(null);

    renderUserNode(userId);

    const button = screen.getByRole('button', { name: 'Generate' });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('title', useGraphStore.getState().sendBlocker(userId));
  });

  it('is offered for an empty user node whose lineage carries a file node', async () => {
    vi.mocked(transport.statVaultFile).mockResolvedValue({
      status: 'ok',
      stat: { size: 120, modifiedEpochMs: 1_000 },
      mimeType: 'text/markdown',
      name: 'plan.md',
    });
    const state = useGraphStore.getState();
    const fileId = state.addFileNode(NOTE, { x: 0, y: 0 });
    const userId = state.createUserNodeDownstream(fileId);
    await useGraphStore.getState().refreshFileNodeStat(fileId);
    useUIStore.getState().setEditing(null);

    renderUserNode(userId);

    expect(screen.getByRole('button', { name: 'Generate' })).toBeEnabled();
    expect(screen.queryByText('Double-click to edit in panel')).not.toBeInTheDocument();
  });

  it('is not offered for an empty user node with nothing to send', () => {
    const userId = useGraphStore.getState().createUserNode();
    useUIStore.getState().setEditing(null);

    renderUserNode(userId);

    expect(screen.queryByRole('button', { name: 'Generate' })).not.toBeInTheDocument();
    expect(screen.getByText('Double-click to edit in panel')).toBeInTheDocument();
  });
});
