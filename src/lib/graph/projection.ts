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
function lineageOf(g: Graph, selectedNodeId: NodeId | null): Set<NodeId> {
  if (!selectedNodeId || !g.nodes.has(selectedNodeId)) return new Set();
  const lineage = GraphModel.ancestors(g, selectedNodeId);
  lineage.delete(selectedNodeId);
  for (const id of lineage) if (!g.nodes.has(id)) lineage.delete(id);
  return lineage;
}

/** An edge among the Lineage highlight's nodes, or from one of them into the selected node. */
function inLineage(e: { source: string; target: string }, lineage: Set<NodeId>, selectedNodeId: NodeId | null) {
  return lineage.has(e.source) && (e.target === selectedNodeId || lineage.has(e.target));
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

/**
 * The node the Lineage highlight belongs to: the only node ReactFlow has
 * selected. A selection box can select several, and then there is none.
 */
export function lineageNodeId(nodes: FlowNode[]): NodeId | null {
  const selected = nodes.filter((n) => n.selected);
  return selected.length === 1 ? selected[0].id : null;
}

/**
 * Re-mark the Lineage highlight on nodes and edges ReactFlow already holds,
 * keeping their own state (selection, measurements) intact.
 */
export function withLineage(
  flow: { nodes: FlowNode[]; edges: Edge[] },
  g: Graph,
  selectedNodeId: NodeId | null
): { nodes: FlowNode[]; edges: Edge[] } {
  const lineage = lineageOf(g, selectedNodeId);
  const mark = <T extends { className?: string }>(item: T, on: boolean): T => {
    const className = on ? LINEAGE_CLASS : undefined;
    return item.className === className ? item : { ...item, className };
  };
  return {
    nodes: flow.nodes.map((n) => mark(n, lineage.has(n.id))),
    edges: flow.edges.map((e) => mark(e, inLineage(e, lineage, selectedNodeId))),
  };
}

export function graphToFlowEdges(g: Graph, ui: UiState): Edge[] {
  const lineage = lineageOf(g, ui.selectedNodeId);
  return g.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    type: FLOATING_EDGE,
    ...(inLineage(e, lineage, ui.selectedNodeId) && { className: LINEAGE_CLASS }),
  }));
}
