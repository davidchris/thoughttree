import { describe, expect, it, beforeEach } from 'vitest';
import { useGraphStore } from './useGraphStore';

describe('useGraphStore delete flow', () => {
  beforeEach(() => {
    useGraphStore.getState().newProject();
  });

  it('deleteNode action removes node, edges, and updates nodes/nodeData', () => {
    const s = useGraphStore.getState();
    const a = s.createUserNode({ x: 0, y: 0 });
    const b = s.createAgentNodeDownstream(a, 'claude-code');

    expect(useGraphStore.getState().graph.nodes.size).toBe(2);
    expect(useGraphStore.getState().nodes.length).toBe(2);
    expect(useGraphStore.getState().edges.length).toBe(1);

    useGraphStore.getState().deleteNode(b);

    const s2 = useGraphStore.getState();
    expect(s2.graph.nodes.has(b)).toBe(false);
    expect(s2.nodes.find((n) => n.id === b)).toBeUndefined();
    expect(s2.nodeData.has(b)).toBe(false);
    expect(s2.edges.length).toBe(0);
  });

  it('onNodesChange remove handles ReactFlow Backspace/Delete keypress', () => {
    const s = useGraphStore.getState();
    const a = s.createUserNode({ x: 0, y: 0 });

    useGraphStore.getState().onNodesChange([{ type: 'remove', id: a }]);

    const s2 = useGraphStore.getState();
    expect(s2.graph.nodes.has(a)).toBe(false);
    expect(s2.nodes.find((n) => n.id === a)).toBeUndefined();
    expect(s2.nodeData.has(a)).toBe(false);
    expect(s2.selectedNodeId).toBeNull();
    expect(s2.editingNodeId).toBeNull();
  });

  it('projects selected:true on the selected node so ReactFlow keyboard delete works', () => {
    const s = useGraphStore.getState();
    const a = s.createUserNode({ x: 0, y: 0 });
    const flowNode = useGraphStore.getState().nodes.find((n) => n.id === a)!;
    expect(flowNode.selected).toBe(true);
  });

  it('deleting selected node clears selectedNodeId', () => {
    const s = useGraphStore.getState();
    const a = s.createUserNode({ x: 0, y: 0 });
    expect(useGraphStore.getState().selectedNodeId).toBe(a);
    useGraphStore.getState().deleteNode(a);
    expect(useGraphStore.getState().selectedNodeId).toBeNull();
  });
});
