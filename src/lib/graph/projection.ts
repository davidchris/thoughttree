import type { Edge, Node } from '@xyflow/react';
import { GraphModel, type Graph, type GraphNode, type NodeId } from '@thoughttree/graph-model';

export interface FlowNodeData extends Record<string, unknown> {
  id: NodeId;
  isSelected: boolean;
}

export type FlowNode = Node<FlowNodeData>;

interface UiState {
  selectedNodeId: NodeId | null;
}

const DEFAULT_POSITION = { x: 0, y: 0 };

/** Class on the ancestors of the selected node and the edges between them. */
const LINEAGE_CLASS = 'lineage';

// ReactFlow node type registered for each GraphNode role (see Graph/index.tsx nodeTypes).
const FLOW_TYPE_BY_ROLE: Record<GraphNode['role'], string> = {
  user: 'user',
  assistant: 'agent',
  file: 'file',
};

/**
 * Everything that feeds into the selected node: all ancestors through every
 * parent, so both sides of a merge count. The selected node itself is not
 * included, even when a legacy cycle makes it its own ancestor.
 */
function lineageOf(g: Graph, selectedNodeId: NodeId | null): Set<NodeId> {
  if (!selectedNodeId || !g.nodes.has(selectedNodeId)) return new Set();
  const lineage = GraphModel.ancestors(g, selectedNodeId);
  lineage.delete(selectedNodeId);
  return lineage;
}

export function graphToFlowNodes(g: Graph, ui: UiState): FlowNode[] {
  const lineage = lineageOf(g, ui.selectedNodeId);
  const result: FlowNode[] = [];
  for (const node of g.nodes.values()) {
    const position = g.layout.get(node.id) ?? DEFAULT_POSITION;
    const isSelected = ui.selectedNodeId === node.id;
    result.push({
      id: node.id,
      type: FLOW_TYPE_BY_ROLE[node.role],
      position,
      selected: isSelected,
      data: { id: node.id, isSelected },
      dragHandle: '.thought-node',
      ...(lineage.has(node.id) && { className: LINEAGE_CLASS }),
    });
  }
  return result;
}

/** ReactFlow edge type that attaches to whichever sides face each other. */
export const FLOATING_EDGE = 'floating';

export function graphToFlowEdges(g: Graph, ui: UiState): Edge[] {
  const lineage = lineageOf(g, ui.selectedNodeId);
  return g.edges.map((e) => {
    const intoSelection = e.target === ui.selectedNodeId || lineage.has(e.target);
    return {
      id: e.id,
      source: e.source,
      target: e.target,
      type: FLOATING_EDGE,
      ...(intoSelection && lineage.has(e.source) && { className: LINEAGE_CLASS }),
    };
  });
}
