import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { NodeChange } from '@xyflow/react';
import { invoke } from '@tauri-apps/api/core';
import { GRAPH_JSON_VERSION, GraphMutations, GraphSerialize } from '@thoughttree/graph-model';
import { STREAM_FLUSH_INTERVAL_MS, useGraphStore } from './useGraphStore';
import { useProviderStore } from './useProviderStore';
import { useUIStore } from './useUIStore';
import { hasFreshSummary } from '../hooks/useSummaryGeneration';

function resetStore() {
  const state = useGraphStore.getState();
  state.newProject();
  useGraphStore.setState({
    selectedNodeId: null,
    streamingNodeIds: new Set<string>(),
    isDirty: false,
  });
  useProviderStore.setState({
    globalModelPreferences: {},
    globalEffortPreferences: {},
  });
  useUIStore.getState().reset();
}

const mockedInvoke = vi.mocked(invoke);

describe('useGraphStore', () => {
  beforeEach(() => {
    vi.useRealTimers();
    mockedInvoke.mockReset();
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

  it('resolves project reasoning effort before global without touching model preferences', () => {
    useProviderStore.getState().setGlobalEffortPreferences({ codex: 'medium' });
    useProviderStore.getState().setGlobalModelPreferences({ codex: 'gpt-5.5' });

    expect(useGraphStore.getState().getEffectiveEffort('codex')).toBe('medium');
    expect(useGraphStore.getState().getEffectiveEffort('claude-code')).toBeUndefined();

    useGraphStore.getState().setProjectEffortPreference('codex', 'xhigh');

    expect(useGraphStore.getState().getEffectiveEffort('codex')).toBe('xhigh');
    expect(useGraphStore.getState().getEffectiveModel('codex')).toBe('gpt-5.5');
  });

  it('round-trips project reasoning effort preferences in V3 files', async () => {
    mockedInvoke.mockResolvedValue(undefined);
    useGraphStore.setState({ projectPath: '/tmp/project.thoughttree' });
    useGraphStore.getState().setProjectEffortPreference('claude-code', 'high');

    await useGraphStore.getState().saveProject();

    const saveCall = mockedInvoke.mock.calls.find(([command]) => command === 'save_project');
    expect(saveCall).toBeDefined();
    const saved = JSON.parse((saveCall?.[1] as { data: string }).data);
    expect(saved.version).toBe(3);
    expect(saved.projectEffortPreferences).toEqual({ 'claude-code': 'high' });

    const graph = GraphSerialize.toJSON(GraphMutations.empty());
    mockedInvoke.mockImplementation(async (command) => {
      if (command === 'load_project') {
        return JSON.stringify({
          version: GRAPH_JSON_VERSION,
          graph,
          projectEffortPreferences: { codex: 'xhigh', 'claude-code': null },
        });
      }
      return undefined;
    });

    await useGraphStore.getState().loadProject('/tmp/project.thoughttree');

    expect(useGraphStore.getState().projectEffortPreferences).toEqual({ codex: 'xhigh' });

    mockedInvoke.mockImplementation(async (command) => {
      if (command === 'load_project') {
        return JSON.stringify({ version: GRAPH_JSON_VERSION, graph });
      }
      return undefined;
    });

    await useGraphStore.getState().loadProject('/tmp/project-without-effort.thoughttree');
    expect(useGraphStore.getState().projectEffortPreferences).toBeNull();

    useGraphStore.getState().setProjectEffortPreference('codex', 'low');
    useGraphStore.getState().newProject();
    expect(useGraphStore.getState().projectEffortPreferences).toBeNull();
  });
});

describe('streaming chunk batching', () => {
  function createStreamingAgentNode(): string {
    const userId = useGraphStore.getState().createUserNode();
    return useGraphStore.getState().createAgentNodeDownstream(userId);
  }

  function nodeContent(nodeId: string): string {
    return useGraphStore.getState().graph.nodes.get(nodeId)?.content ?? '';
  }

  beforeEach(() => {
    vi.useFakeTimers();
    resetStore();
  });

  afterEach(() => {
    // Drain pending flush so buffered chunks don't leak across tests
    vi.advanceTimersByTime(STREAM_FLUSH_INTERVAL_MS);
    vi.useRealTimers();
  });

  it('buffers chunks and applies them after the flush interval', () => {
    const agentId = createStreamingAgentNode();

    useGraphStore.getState().appendToNode(agentId, 'Hello');
    useGraphStore.getState().appendToNode(agentId, ' world');

    expect(nodeContent(agentId)).toBe('');

    vi.advanceTimersByTime(STREAM_FLUSH_INTERVAL_MS);

    expect(nodeContent(agentId)).toBe('Hello world');
  });

  it('projects the graph once per flush, not once per chunk', () => {
    const agentId = createStreamingAgentNode();
    const graphBefore = useGraphStore.getState().graph;

    for (let i = 0; i < 50; i++) {
      useGraphStore.getState().appendToNode(agentId, `chunk${i} `);
    }

    expect(useGraphStore.getState().graph).toBe(graphBefore);

    vi.advanceTimersByTime(STREAM_FLUSH_INTERVAL_MS);

    expect(useGraphStore.getState().graph).not.toBe(graphBefore);
    expect(nodeContent(agentId)).toContain('chunk0 ');
    expect(nodeContent(agentId)).toContain('chunk49 ');
  });

  it('stopStreaming flushes buffered chunks immediately', () => {
    const agentId = createStreamingAgentNode();

    useGraphStore.getState().appendToNode(agentId, 'final tail');
    useGraphStore.getState().stopStreaming(agentId);

    expect(nodeContent(agentId)).toBe('final tail');
    expect(useGraphStore.getState().streamingNodeIds.has(agentId)).toBe(false);
  });

  it('buildConversationContext sees buffered chunks', () => {
    const agentId = createStreamingAgentNode();

    useGraphStore.getState().appendToNode(agentId, 'partial answer');
    const context = useGraphStore.getState().buildConversationContext(agentId);

    expect(context.some((m) => m.content === 'partial answer')).toBe(true);
  });

  it('drops buffered chunks for a deleted node without error', () => {
    const agentId = createStreamingAgentNode();

    useGraphStore.getState().appendToNode(agentId, 'never lands');
    useGraphStore.getState().deleteNode(agentId);

    expect(() => vi.advanceTimersByTime(STREAM_FLUSH_INTERVAL_MS)).not.toThrow();
    expect(useGraphStore.getState().graph.nodes.has(agentId)).toBe(false);
  });

  it('keeps interleaved streams to two nodes separate', () => {
    const agentA = createStreamingAgentNode();
    const agentB = createStreamingAgentNode();

    useGraphStore.getState().appendToNode(agentA, 'aaa');
    useGraphStore.getState().appendToNode(agentB, 'bbb');
    useGraphStore.getState().appendToNode(agentA, 'AAA');

    vi.advanceTimersByTime(STREAM_FLUSH_INTERVAL_MS);

    expect(nodeContent(agentA)).toBe('aaaAAA');
    expect(nodeContent(agentB)).toBe('bbb');
  });
});
