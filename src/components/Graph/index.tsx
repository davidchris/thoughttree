import { useCallback, useRef, useEffect, useState } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  NodeTypes,
  useReactFlow,
  addEdge,
  type OnConnectEnd,
  type Connection,
  type NodeChange,
  type Node,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { UserNode } from './UserNode';
import { AgentNode } from './AgentNode';
import { ContextMenu } from './ContextMenu';
import { AlignmentGuides, type AlignmentGuide } from './AlignmentGuides';
import { useGraphStore } from '../../store/useGraphStore';
import './styles.css';

const SNAP_THRESHOLD = 8;
const DEFAULT_NODE_SIZE = 120;

const nodeTypes: NodeTypes = {
  user: UserNode,
  agent: AgentNode,
};

export function Graph() {
  const {
    nodes,
    edges,
    onNodesChange,
    onEdgesChange,
    onConnect,
    selectNode,
    createUserNode,
    setEditing,
    selectedNodeId,
    nodeData,
    createUserNodeDownstream,
    isNodeBlocked,
    editingNodeId,
    togglePreviewNode,
    setPreviewNode,
    previewNodeId,
    triggerSidePanelEditMode,
  } = useGraphStore();
  const { screenToFlowPosition } = useReactFlow();
  const connectingNodeId = useRef<string | null>(null);

  // Context menu state
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    nodeId: string;
  } | null>(null);

  // Alignment guides state
  const [alignmentGuides, setAlignmentGuides] = useState<AlignmentGuide[]>([]);

  // Get node dimensions (use measured if available, otherwise default)
  const getNodeDimensions = useCallback((node: Node) => {
    const width = node.measured?.width ?? DEFAULT_NODE_SIZE;
    const height = node.measured?.height ?? DEFAULT_NODE_SIZE;
    return { width, height };
  }, []);

  // Calculate alignment guides and snapped position for a dragging node
  const calculateAlignments = useCallback(
    (draggingNode: Node, allNodes: Node[]) => {
      const guides: AlignmentGuide[] = [];
      let snappedX = draggingNode.position.x;
      let snappedY = draggingNode.position.y;

      const dragDims = getNodeDimensions(draggingNode);
      const dragLeft = draggingNode.position.x;
      const dragRight = dragLeft + dragDims.width;
      const dragCenterX = dragLeft + dragDims.width / 2;
      const dragTop = draggingNode.position.y;
      const dragBottom = dragTop + dragDims.height;
      const dragCenterY = dragTop + dragDims.height / 2;

      for (const node of allNodes) {
        if (node.id === draggingNode.id) continue;

        const nodeDims = getNodeDimensions(node);
        const nodeLeft = node.position.x;
        const nodeRight = nodeLeft + nodeDims.width;
        const nodeCenterX = nodeLeft + nodeDims.width / 2;
        const nodeTop = node.position.y;
        const nodeBottom = nodeTop + nodeDims.height;
        const nodeCenterY = nodeTop + nodeDims.height / 2;

        // Vertical alignments (check X positions)
        // Left edge to left edge
        if (Math.abs(dragLeft - nodeLeft) < SNAP_THRESHOLD) {
          snappedX = nodeLeft;
          guides.push({ type: 'vertical', position: nodeLeft });
        }
        // Right edge to right edge
        else if (Math.abs(dragRight - nodeRight) < SNAP_THRESHOLD) {
          snappedX = nodeRight - dragDims.width;
          guides.push({ type: 'vertical', position: nodeRight });
        }
        // Center X to center X
        else if (Math.abs(dragCenterX - nodeCenterX) < SNAP_THRESHOLD) {
          snappedX = nodeCenterX - dragDims.width / 2;
          guides.push({ type: 'vertical', position: nodeCenterX });
        }
        // Left edge to right edge (adjacent horizontal)
        else if (Math.abs(dragLeft - nodeRight) < SNAP_THRESHOLD) {
          snappedX = nodeRight;
          guides.push({ type: 'vertical', position: nodeRight });
        }
        // Right edge to left edge (adjacent horizontal)
        else if (Math.abs(dragRight - nodeLeft) < SNAP_THRESHOLD) {
          snappedX = nodeLeft - dragDims.width;
          guides.push({ type: 'vertical', position: nodeLeft });
        }

        // Horizontal alignments (check Y positions)
        // Top edge to top edge
        if (Math.abs(dragTop - nodeTop) < SNAP_THRESHOLD) {
          snappedY = nodeTop;
          guides.push({ type: 'horizontal', position: nodeTop });
        }
        // Bottom edge to bottom edge
        else if (Math.abs(dragBottom - nodeBottom) < SNAP_THRESHOLD) {
          snappedY = nodeBottom - dragDims.height;
          guides.push({ type: 'horizontal', position: nodeBottom });
        }
        // Center Y to center Y
        else if (Math.abs(dragCenterY - nodeCenterY) < SNAP_THRESHOLD) {
          snappedY = nodeCenterY - dragDims.height / 2;
          guides.push({ type: 'horizontal', position: nodeCenterY });
        }
        // Top edge to bottom edge (adjacent vertical)
        else if (Math.abs(dragTop - nodeBottom) < SNAP_THRESHOLD) {
          snappedY = nodeBottom;
          guides.push({ type: 'horizontal', position: nodeBottom });
        }
        // Bottom edge to top edge (adjacent vertical)
        else if (Math.abs(dragBottom - nodeTop) < SNAP_THRESHOLD) {
          snappedY = nodeTop - dragDims.height;
          guides.push({ type: 'horizontal', position: nodeTop });
        }
      }

      // Deduplicate guides by position
      const uniqueGuides = guides.filter(
        (guide, index, self) =>
          self.findIndex(
            (g) => g.type === guide.type && g.position === guide.position
          ) === index
      );

      return {
        snappedPosition: { x: snappedX, y: snappedY },
        guides: uniqueGuides,
      };
    },
    [getNodeDimensions]
  );

  // Custom onNodesChange handler that adds snapping behavior
  const handleNodesChange = useCallback(
    (changes: NodeChange[]) => {
      let modifiedChanges = changes;
      let newGuides: AlignmentGuide[] = [];

      // Check for dragging nodes
      const positionChanges = changes.filter(
        (change): change is NodeChange & { type: 'position'; dragging?: boolean; position?: { x: number; y: number } } =>
          change.type === 'position'
      );

      const draggingChange = positionChanges.find((c) => c.dragging && c.position);

      if (draggingChange && draggingChange.position) {
        // Find the dragging node
        const draggingNode = nodes.find((n) => n.id === draggingChange.id);
        if (draggingNode) {
          // Create a temporary node with the new position for calculation
          const tempNode = {
            ...draggingNode,
            position: draggingChange.position,
          };

          const { snappedPosition, guides } = calculateAlignments(tempNode, nodes);
          newGuides = guides;

          // Update the position change with snapped position
          modifiedChanges = changes.map((change) => {
            if (change.type === 'position' && change.id === draggingChange.id && change.position) {
              return {
                ...change,
                position: snappedPosition,
              };
            }
            return change;
          });
        }
      } else {
        // Check for drag end (dragging === false with a position)
        const dragEndChange = positionChanges.find((c) => c.dragging === false && c.position);

        if (dragEndChange && dragEndChange.position) {
          // Apply snapping to the final position on drag end
          const dragEndNode = nodes.find((n) => n.id === dragEndChange.id);
          if (dragEndNode) {
            const tempNode = {
              ...dragEndNode,
              position: dragEndChange.position,
            };

            const { snappedPosition } = calculateAlignments(tempNode, nodes);

            // Update the final position with snapped position
            modifiedChanges = changes.map((change) => {
              if (change.type === 'position' && change.id === dragEndChange.id && change.position) {
                return {
                  ...change,
                  position: snappedPosition,
                };
              }
              return change;
            });
          }
          // Clear guides on drag end
          newGuides = [];
        }
      }

      setAlignmentGuides(newGuides);
      onNodesChange(modifiedChanges);
    },
    [nodes, onNodesChange, calculateAlignments]
  );

  const onNodeContextMenu = useCallback(
    (event: React.MouseEvent, node: { id: string }) => {
      event.preventDefault();
      setContextMenu({ x: event.clientX, y: event.clientY, nodeId: node.id });
    },
    []
  );

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Spacebar to toggle preview panel (but not when typing in an input)
      if (e.key === ' ' && selectedNodeId && !editingNodeId) {
        const target = e.target as HTMLElement;
        if (target.tagName === 'TEXTAREA' || target.tagName === 'INPUT' || target.isContentEditable) {
          return; // Let the input handle the space
        }
        e.preventDefault();
        togglePreviewNode(selectedNodeId);
        return;
      }

      // "E" to enter edit mode on side panel (when previewing a user node)
      if (e.key.toLowerCase() === 'e' && previewNodeId && !editingNodeId && !isNodeBlocked(previewNodeId)) {
        const target = e.target as HTMLElement;
        if (target.tagName === 'TEXTAREA' || target.tagName === 'INPUT' || target.isContentEditable) {
          return; // Let the input handle the keystroke
        }
        const previewData = nodeData.get(previewNodeId);
        if (previewData?.role === 'user') {
          e.preventDefault();
          triggerSidePanelEditMode();
          return;
        }
      }

      // Don't trigger other shortcuts if editing
      if (editingNodeId) return;

      // Check if selected node is an agent node for Enter shortcut
      if (!selectedNodeId) return;
      const data = nodeData.get(selectedNodeId);
      if (!data || data.role !== 'assistant') return;

      // Block if this node is in a streaming lineage
      if (isNodeBlocked(selectedNodeId)) return;

      // Enter to reply
      if (e.key === 'Enter') {
        e.preventDefault();
        createUserNodeDownstream(selectedNodeId);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [selectedNodeId, nodeData, createUserNodeDownstream, isNodeBlocked, editingNodeId, togglePreviewNode, previewNodeId, triggerSidePanelEditMode]);

  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: { id: string }) => {
      selectNode(node.id);
    },
    [selectNode]
  );

  const onPaneClick = useCallback(
    (event: React.MouseEvent) => {
      if (event.detail === 2) {
        // Double-click: create new user node
        const position = screenToFlowPosition({ x: event.clientX, y: event.clientY });
        createUserNode(position);
      } else {
        // Single-click: deselect, exit editing, and close preview
        selectNode(null);
        setEditing(null);
        setPreviewNode(null);
      }
    },
    [screenToFlowPosition, createUserNode, selectNode, setEditing, setPreviewNode]
  );

  const onConnectStart = useCallback(
    (_event: unknown, params: { nodeId: string | null }) => {
      connectingNodeId.current = params.nodeId;
    },
    []
  );

  const onConnectEnd: OnConnectEnd = useCallback(
    (event) => {
      if (!connectingNodeId.current) return;

      // Check if we dropped on a valid target (another node's handle)
      const targetIsPane = (event.target as Element).classList.contains('react-flow__pane');

      if (targetIsPane) {
        // Dropped on canvas - create new user node
        const { clientX, clientY } = event instanceof MouseEvent ? event : event.changedTouches[0];
        const position = screenToFlowPosition({ x: clientX, y: clientY });

        // Create new user node at drop position
        const newNodeId = createUserNode(position);

        // Create edge from source to new node
        useGraphStore.setState((state) => ({
          edges: addEdge(
            {
              id: `${connectingNodeId.current}-${newNodeId}`,
              source: connectingNodeId.current!,
              target: newNodeId,
            },
            state.edges
          ),
        }));
      }

      connectingNodeId.current = null;
    },
    [screenToFlowPosition, createUserNode]
  );

  // Wrap onConnect to handle edge direction when connecting to agent nodes
  // If user drags from a user node TO an agent node, reverse the direction
  // so the agent becomes the parent (source) and user node becomes child (target)
  const handleConnect = useCallback(
    (connection: Connection) => {
      const targetNodeData = connection.target ? nodeData.get(connection.target) : null;
      const sourceNodeData = connection.source ? nodeData.get(connection.source) : null;

      // If connecting TO an agent node FROM a user node, reverse direction
      if (targetNodeData?.role === 'assistant' && sourceNodeData?.role === 'user') {
        const reversedConnection: Connection = {
          source: connection.target,
          target: connection.source,
          sourceHandle: connection.targetHandle,
          targetHandle: connection.sourceHandle,
        };
        onConnect(reversedConnection);
      } else {
        onConnect(connection);
      }
    },
    [onConnect, nodeData]
  );

  return (
    <div className="graph-container">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={handleNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={handleConnect}
        onConnectStart={onConnectStart}
        onConnectEnd={onConnectEnd}
        onNodeClick={onNodeClick}
        onPaneClick={onPaneClick}
        onNodeContextMenu={onNodeContextMenu}
        nodeTypes={nodeTypes}
        fitView
        zoomOnDoubleClick={false}
        proOptions={{ hideAttribution: true }}
      >
        <Background color="#333" gap={20} />
        <Controls />
        <MiniMap
          nodeColor={(node) => node.type === 'user' ? '#3b82f6' : '#22c55e'}
          maskColor="rgba(0,0,0,0.8)"
        />
        <AlignmentGuides guides={alignmentGuides} />
      </ReactFlow>
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          nodeId={contextMenu.nodeId}
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
}