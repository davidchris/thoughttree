import type { Edge, Node } from '@xyflow/react';
import { GraphModel, type Graph, type GraphNode, type NodeId } from '@thoughttree/graph-model';

export interface FlowNodeData extends Record<string, unknown> {
  id: NodeId;
  isSelected: boolean;
}

export type FlowNode = Node<FlowNodeData>;

interface UiState {
  selectedNodeId: NodeId | null;
  /** More than one node is selected (e.g. with a selection box): no Lineage highlight. */
  multiSelection?: boolean;
}

const DEFAULT_POSITION = { x: 0, y: 0 };

/** Lineage highlight class: on the selected node's ancestors and the edges among them and into it. */
const LINEAGE_CLASS = 'lineage';

// ReactFlow node type registered for each GraphNode role (see Graph/index.tsx nodeTypes).
const FLOW_TYPE_BY_ROLE: Record<GraphNode['role'], string> = {
  user: 'user',
  assistant: 'agent',
  file: 'file',
};

/**
 * Everything that feeds into the selected node: all ancestors through every
 * parent, so both sides of a Synthesizer node count. The selected node itself
 * is not included, even when a legacy cycle makes it its own ancestor.
 */
function lineageOf(g: Graph, ui: UiState): Set<NodeId> {
  const { selectedNodeId } = ui;
  if (ui.multiSelection || !selectedNodeId || !g.nodes.has(selectedNodeId)) return new Set();
  const lineage = GraphModel.ancestors(g, selectedNodeId);
  lineage.delete(selectedNodeId);
  for (const id of lineage) if (!g.nodes.has(id)) lineage.delete(id);
  return lineage;
}

export function graphToFlowNodes(g: Graph, ui: UiState): FlowNode[] {
  const lineage = lineageOf(g, ui);
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

/**
 * Re-mark the Lineage highlight on nodes ReactFlow already holds, keeping its
 * own state (selection, measurements) intact.
 */
export function withLineage(nodes: FlowNode[], g: Graph, ui: UiState): FlowNode[] {
  const lineage = lineageOf(g, ui);
  return nodes.map((n) => {
    const className = lineage.has(n.id) ? LINEAGE_CLASS : undefined;
    return n.className === className ? n : { ...n, className };
  });
}

export function graphToFlowEdges(g: Graph, ui: UiState): Edge[] {
  const lineage = lineageOf(g, ui);
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
