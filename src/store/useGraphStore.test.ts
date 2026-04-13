import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { NodeChange } from '@xyflow/react';
import { useGraphStore } from './useGraphStore';
import { hasFreshSummary } from '../hooks/useSummaryGeneration';

function resetStore() {
  const state = useGraphStore.getState();
  state.newProject();
  useGraphStore.setState({
    selectedNodeId: null,
    editingNodeId: null,
    previewNodeId: null,
    streamingNodeIds: new Set<string>(),
    pendingPermission: null,
    triggerSidePanelEdit: false,
    isDirty: false,
  });
}

describe('useGraphStore', () => {
  beforeEach(() => {
    vi.useRealTimers();
    resetStore();
  });

  it('does not mark dirty for selection-only node changes', () => {
    const state = useGraphStore.getState();
    const id = state.createUserNode();
    useGraphStore.setState({ isDirty: false });

    const selectChange: NodeChange = {
      id,
      type: 'select',
      selected: true,
    };
    state.onNodesChange([selectChange]);
    expect(useGraphStore.getState().isDirty).toBe(false);
  });

  it('marks dirty for positional node changes', () => {
    const state = useGraphStore.getState();
    const id = state.createUserNode();
    useGraphStore.setState({ isDirty: false });

    const positionChange: NodeChange = {
      id,
      type: 'position',
      position: { x: 150, y: 250 },
      dragging: false,
    };
    state.onNodesChange([positionChange]);
    expect(useGraphStore.getState().isDirty).toBe(true);
  });

  it('updates contentUpdatedAt and invalidates stale summaries after edits', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-01-01T00:00:00.000Z'));

    const state = useGraphStore.getState();
    const id = state.createUserNode();

    vi.setSystemTime(new Date('2026-01-01T00:00:01.000Z'));
    state.updateNodeContent(id, 'Initial content');
    state.setSummary(id, 'Initial summary');
    const summaryTimestamp = useGraphStore.getState().nodeData.get(id)?.summaryTimestamp ?? 0;

    vi.setSystemTime(new Date('2026-01-01T00:00:02.000Z'));
    state.updateNodeContent(id, 'Edited content');

    const updated = useGraphStore.getState().nodeData.get(id);
    expect(updated).toBeDefined();
    expect(updated?.contentUpdatedAt).toBeGreaterThan(summaryTimestamp);
    expect(updated?.summary).toBe('Initial summary');
    expect(hasFreshSummary(updated!)).toBe(false);
  });

  it('returns exact conversation path IDs even with duplicate content', () => {
    const state = useGraphStore.getState();
    const rootUserId = state.createUserNode();
    state.updateNodeContent(rootUserId, 'Same content');

    const agentId = state.createAgentNodeDownstream(rootUserId);
    state.stopStreaming(agentId);
    state.updateNodeContent(agentId, 'Same content');

    const branchUserId = state.createUserNodeDownstream(rootUserId);
    state.updateNodeContent(branchUserId, 'Same content');

    const pathToBranch = state.getConversationPathNodeIds(branchUserId);
    expect(pathToBranch).toEqual([rootUserId, branchUserId]);

    const exported = state.exportSubgraph(pathToBranch);
    expect(exported).toContain('## User');
    expect(exported).not.toContain('## Assistant');
  });
});
