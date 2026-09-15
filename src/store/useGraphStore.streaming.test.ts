import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useGraphStore, STREAM_FLUSH_INTERVAL_MS } from './useGraphStore';

describe('Turn ownership', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useGraphStore.getState().newProject();
  });

  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  it('accepts chunks only from the node’s registered Turn', () => {
    const store = useGraphStore.getState();
    const user = store.createUserNode();
    const node = store.createAgentNodeDownstream(user);
    const turn = useGraphStore.getState().activeTurns.get(node)!;

    store.appendToNode(node, turn, 'First recommendation');
    store.appendToNode(node, 'another-turn', 'Second recommendation');
    vi.advanceTimersByTime(STREAM_FLUSH_INTERVAL_MS);

    expect(useGraphStore.getState().graph.nodes.get(node)?.content).toBe('First recommendation');
  });

  it('discards buffered and late chunks after replacing a graph with the same node IDs', () => {
    const store = useGraphStore.getState();
    const node = store.createAgentNodeDownstream(store.createUserNode());
    const turn = useGraphStore.getState().activeTurns.get(node)!;
    const graph = useGraphStore.getState().graph;
    store.appendToNode(node, turn, 'Buffered old answer');

    store.importGraph('Reloaded graph', graph);
    store.appendToNode(node, turn, 'Late old answer');
    store.stopStreaming(node, turn);
    vi.advanceTimersByTime(STREAM_FLUSH_INTERVAL_MS);

    expect(useGraphStore.getState().graph.nodes.get(node)?.content).toBe('');
    expect(useGraphStore.getState().activeTurns.size).toBe(0);
  });

  it('rejects a duplicate registration and ignores a different Turn’s completion', () => {
    const store = useGraphStore.getState();
    const node = store.createAgentNodeDownstream(store.createUserNode());
    const turn = useGraphStore.getState().activeTurns.get(node)!;

    expect(() => store.startStreaming(node)).toThrow('already has an active Turn');
    store.stopStreaming(node, 'other-turn');
    store.appendToNode(node, turn, 'Still streaming');
    store.stopStreaming(node, turn);
    store.appendToNode(node, turn, 'Late chunk after completion');
    vi.advanceTimersByTime(STREAM_FLUSH_INTERVAL_MS);

    expect(useGraphStore.getState().graph.nodes.get(node)?.content).toBe('Still streaming');
    expect(useGraphStore.getState().activeTurns.has(node)).toBe(false);
  });

  it('ignores old chunks and completion while a later Turn owns the node', () => {
    const store = useGraphStore.getState();
    const node = store.createAgentNodeDownstream(store.createUserNode());
    const first = useGraphStore.getState().activeTurns.get(node)!;
    store.stopStreaming(node, first);
    const second = store.startStreaming(node);
    expect(second).not.toBe(first);

    store.appendToNode(node, first, 'Old response');
    store.stopStreaming(node, first);
    store.appendToNode(node, second, 'Current response');
    vi.advanceTimersByTime(STREAM_FLUSH_INTERVAL_MS);

    expect(useGraphStore.getState().graph.nodes.get(node)?.content).toBe('Current response');
    expect(useGraphStore.getState().activeTurns.get(node)).toBe(second);
  });

  it.each(['deleteNode', 'onNodesChange'] as const)('revokes ownership on %s', (remove) => {
    const store = useGraphStore.getState();
    const node = store.createAgentNodeDownstream(store.createUserNode());
    const turn = useGraphStore.getState().activeTurns.get(node)!;
    store.appendToNode(node, turn, 'Buffered response');
    if (remove === 'deleteNode') store.deleteNode(node);
    else store.onNodesChange([{ type: 'remove', id: node }]);
    store.appendToNode(node, turn, 'Late response');
    store.stopStreaming(node, turn);
    vi.advanceTimersByTime(STREAM_FLUSH_INTERVAL_MS);

    expect(useGraphStore.getState().graph.nodes.has(node)).toBe(false);
    expect(useGraphStore.getState().activeTurns.has(node)).toBe(false);
  });
});
