import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
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

  it('registers Turn identity before sending and drains its final chunks on completion', async () => {
    const store = useGraphStore.getState();
    const user = store.createUserNode();
    store.updateNodeContent(user, 'Recommend a university');
    vi.mocked(transport.sendPrompt).mockImplementation(({ nodeId, turnId }) => {
      expect(turnId).toMatch(/^[0-9a-f-]{36}$/);
      expect(useGraphStore.getState().activeTurns.get(nodeId)).toBe(turnId);
      store.appendToNode(nodeId, turnId, 'The recommendation');
      return Promise.resolve('done');
    });
    const { result } = renderHook(() => useNodeGeneration());
    let node: string | null = null;
    await act(async () => { node = await result.current({ userNodeId: user }); });

    expect(useGraphStore.getState().graph.nodes.get(node!)?.content).toBe('The recommendation');
    expect(useGraphStore.getState().activeTurns.size).toBe(0);
  });

  it('does not let a failed old invocation append an error or finish a newer Turn', async () => {
    const store = useGraphStore.getState();
    const user = store.createUserNode();
    store.updateNodeContent(user, 'Recommend a university');
    let fail!: (error: Error) => void;
    vi.mocked(transport.sendPrompt).mockImplementation(() => new Promise((_, reject) => { fail = reject; }));
    const { result } = renderHook(() => useNodeGeneration());
    let sending!: Promise<string | null>;
    act(() => { sending = result.current({ userNodeId: user }); });
    const [{ nodeId }] = vi.mocked(transport.sendPrompt).mock.calls[0];
    let newer!: string;
    await act(async () => {
      store.importGraph('Reloaded', useGraphStore.getState().graph);
      newer = store.startStreaming(nodeId);
      fail(new Error('Old invocation failed'));
      await sending;
    });

    expect(useGraphStore.getState().activeTurns.get(nodeId)).toBe(newer);
    expect(useGraphStore.getState().graph.nodes.get(nodeId)?.content).toBe('');
  });

  it('closes the registered Turn if preparation fails before sending', async () => {
    const store = useGraphStore.getState();
    const user = store.createUserNode();
    store.updateNodeContent(user, 'Hello');
    const { result } = renderHook(() => useNodeGeneration());
    let node: string | null = null;
    await act(async () => {
      node = await result.current({
        userNodeId: user,
        onAgentNodeCreated: () => { throw new Error('Preparation failed'); },
      });
    });

    expect(transport.sendPrompt).not.toHaveBeenCalled();
    expect(useGraphStore.getState().activeTurns.size).toBe(0);
    expect(useGraphStore.getState().graph.nodes.get(node!)?.content).toContain('Preparation failed');
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
