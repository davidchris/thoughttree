import type { Edge, Node } from '@xyflow/react';
import type { Graph, NodeId } from './types';

export interface FlowNodeData extends Record<string, unknown> {
  id: NodeId;
  isSelected: boolean;
}

export type FlowNode = Node<FlowNodeData>;

interface UiState {
  selectedNodeId: NodeId | null;
}

const DEFAULT_POSITION = { x: 0, y: 0 };

export function graphToFlowNodes(g: Graph, ui: UiState): FlowNode[] {
  const result: FlowNode[] = [];
  for (const node of g.nodes.values()) {
    const position = g.layout.get(node.id) ?? DEFAULT_POSITION;
    result.push({
      id: node.id,
      type: node.role === 'user' ? 'user' : 'agent',
      position,
      data: { id: node.id, isSelected: ui.selectedNodeId === node.id },
      dragHandle: '.thought-node',
    });
  }
  return result;
}

export function graphToFlowEdges(g: Graph): Edge[] {
  return g.edges.map((e) => ({ id: e.id, source: e.source, target: e.target }));
}
