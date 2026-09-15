import { beforeEach, describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { BackendTransport } from '../lib/transport';
import { setBackendTransport } from '../lib/transport';
import { createMockTransport } from '../test/mockTransport';
import { useGraphStore } from '../store/useGraphStore';
import { useUIStore } from '../store/useUIStore';
import { useNodeGeneration } from './useNodeGeneration';

const NOTE = {
  path: 'notes/plan.md',
  name: 'plan.md',
  mimeType: 'text/markdown',
  size: 120,
  seenMtime: 1_000,
  seenSize: 120,
};

describe('useNodeGeneration send gating', () => {
  let transport: BackendTransport;

  beforeEach(() => {
    transport = createMockTransport({ sendPrompt: vi.fn().mockResolvedValue('') });
    setBackendTransport(transport);
    useGraphStore.getState().newProject();
    useUIStore.getState().reset();
  });

  it('refuses to send while a lineage file node is missing, and says why', async () => {
    vi.mocked(transport.statVaultFile).mockResolvedValue({ status: 'missing' });
    const state = useGraphStore.getState();
    const fileId = state.addFileNode(NOTE, { x: 0, y: 0 });
    const userId = state.createUserNodeDownstream(fileId);
    state.updateNodeContent(userId, 'Summarise the plan');
    await useGraphStore.getState().refreshFileNodeStat(fileId);
    const { result } = renderHook(() => useNodeGeneration());

    const agentId = await result.current({ userNodeId: userId });

    expect(agentId).toBeNull();
    expect(transport.sendPrompt).not.toHaveBeenCalled();
    expect(useGraphStore.getState().graph.nodes.size).toBe(2);
    expect(useUIStore.getState().notice).toMatch(/plan\.md.*missing/i);
  });

  it('sends an empty user node when a lineage file node stands in for the text', async () => {
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
    const { result } = renderHook(() => useNodeGeneration());

    const agentId = await result.current({ userNodeId: userId });

    expect(agentId).not.toBeNull();
    expect(transport.sendPrompt).toHaveBeenCalledWith(
      expect.objectContaining({
        messages: [
          {
            role: 'user',
            content: '',
            files: [{ path: 'notes/plan.md', name: 'plan.md', mimeType: 'text/markdown', size: 120 }],
          },
        ],
      })
    );
  });

  it('still refuses an empty user node with nothing in its lineage', async () => {
    const userId = useGraphStore.getState().createUserNode();
    const { result } = renderHook(() => useNodeGeneration());

    const agentId = await result.current({ userNodeId: userId });

    expect(agentId).toBeNull();
    expect(transport.sendPrompt).not.toHaveBeenCalled();
  });

  it('sends when the lineage file node is present', async () => {
    vi.mocked(transport.statVaultFile).mockResolvedValue({
      status: 'ok',
      stat: { size: 120, modifiedEpochMs: 1_000 },
      mimeType: 'text/markdown',
      name: 'plan.md',
    });
    const state = useGraphStore.getState();
    const fileId = state.addFileNode(NOTE, { x: 0, y: 0 });
    const userId = state.createUserNodeDownstream(fileId);
    state.updateNodeContent(userId, 'Summarise the plan');
    await useGraphStore.getState().refreshFileNodeStat(fileId);
    const { result } = renderHook(() => useNodeGeneration());

    const agentId = await result.current({ userNodeId: userId });

    expect(agentId).not.toBeNull();
    expect(transport.sendPrompt).toHaveBeenCalledTimes(1);
    expect(useUIStore.getState().notice).toBeNull();
  });
});
