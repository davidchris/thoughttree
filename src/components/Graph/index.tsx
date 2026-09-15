import { useCallback, useRef, useEffect, useState } from 'react';
import {
  ReactFlow,
  Controls,
  MiniMap,
  NodeTypes,
  useReactFlow,
  type OnConnectEnd,
  type Connection,
  type NodeChange,
  type Node,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { UserNode } from './UserNode';
import { AgentNode } from './AgentNode';
import { FileNode } from './FileNode';
import { ContextMenu, type ContextMenuTarget } from './ContextMenu';
import { AlignmentGuides, type AlignmentGuide } from './AlignmentGuides';
import { useGraphStore } from '../../store/useGraphStore';
import { useUIStore } from '../../store/useUIStore';
import { getBackendTransport } from '../../lib/transport';
import { isTauriRuntime } from '../../lib/desktop';
import { logger } from '../../lib/logger';
import './styles.css';

const SNAP_THRESHOLD = 8;
// Keep in sync with --tt-node-width / --tt-node-height in design/tokens.css.
const DEFAULT_NODE_WIDTH = 170;
const DEFAULT_NODE_HEIGHT = 120;

// MiniMap paints into an SVG that does not resolve CSS variables reliably.
// Keep in sync with --tt-accent, --tt-surface and --tt-edge in design/tokens.css.
const MINT = '#c0facc';
const SURFACE = '#0a3038';
const FILE_FILL = '#4f7a76';

// Several files dropped at once fan out diagonally instead of stacking.
const DROP_CASCADE_PX = 24;

const nodeTypes: NodeTypes = {
  user: UserNode,
  agent: AgentNode,
  file: FileNode,
};

function minimapColor(node: Node): string {
  if (node.type === 'user') return MINT;
  if (node.type === 'file') return FILE_FILL;
  return SURFACE;
}

type GraphSnapshot = ReturnType<typeof useGraphStore.getState>;
type UISnapshot = ReturnType<typeof useUIStore.getState>;

/** True when the keystroke belongs to a text field and must not be hijacked. */
function isTextEntryTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return (
    el?.tagName === 'TEXTAREA' || el?.tagName === 'INPUT' || el?.isContentEditable === true
  );
}

// Keyboard shortcuts. Each one returns true once it has taken responsibility for
// the event, either by acting on it or by deliberately leaving it to a text field.

/** Spacebar toggles the preview panel for the selected node. */
function togglePreviewShortcut(e: KeyboardEvent, graph: GraphSnapshot, ui: UISnapshot): boolean {
  const { selectedNodeId } = graph;
  if (e.key !== ' ' || !selectedNodeId || ui.editingNodeId) return false;
  if (isTextEntryTarget(e.target)) return true;

  e.preventDefault();
  ui.togglePreviewNode(selectedNodeId);
  return true;
}

/** "E" enters edit mode on an open user node, or opens the panel for the selection. */
function editShortcut(e: KeyboardEvent, graph: GraphSnapshot, ui: UISnapshot): boolean {
  const { previewNodeId, editingNodeId } = ui;
  if (e.key.toLowerCase() !== 'e' || editingNodeId) return false;
  if (isTextEntryTarget(e.target)) return true;

  if (previewNodeId && !graph.isNodeBlocked(previewNodeId)) {
    if (graph.nodeData.get(previewNodeId)?.role === 'user') {
      e.preventDefault();
      ui.triggerSidePanelEditMode();
      return true;
    }
  }

  if (!previewNodeId && graph.selectedNodeId) {
    e.preventDefault();
    ui.setPreviewNode(graph.selectedNodeId);
    return true;
  }

  return false;
}

/** Enter replies to the selected agent node, unless it is in a streaming lineage. */
function replyShortcut(e: KeyboardEvent, graph: GraphSnapshot, ui: UISnapshot): boolean {
  const { selectedNodeId, nodeData, isNodeBlocked, createUserNodeDownstream } = graph;
  if (ui.editingNodeId || e.key !== 'Enter' || !selectedNodeId) return false;
  if (nodeData.get(selectedNodeId)?.role !== 'assistant') return false;
  if (isNodeBlocked(selectedNodeId)) return false;

  e.preventDefault();
  createUserNodeDownstream(selectedNodeId);
  return true;
}

export function Graph() {
  const nodes = useGraphStore((state) => state.nodes);
  const edges = useGraphStore((state) => state.edges);
  const onNodesChange = useGraphStore((state) => state.onNodesChange);
  const onEdgesChange = useGraphStore((state) => state.onEdgesChange);
  const onConnect = useGraphStore((state) => state.onConnect);
  const selectNode = useGraphStore((state) => state.selectNode);
  const createUserNode = useGraphStore((state) => state.createUserNode);
  const setEditing = useUIStore((state) => state.setEditing);
  const setPreviewNode = useUIStore((state) => state.setPreviewNode);
  const { screenToFlowPosition } = useReactFlow();
  const connectingNodeId = useRef<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Context menu state
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    target: ContextMenuTarget;
  } | null>(null);

  // Alignment guides state
  const [alignmentGuides, setAlignmentGuides] = useState<AlignmentGuide[]>([]);

  // Get node dimensions (use measured if available, otherwise default)
  const getNodeDimensions = useCallback((node: Node) => {
    const width = node.measured?.width ?? DEFAULT_NODE_WIDTH;
    const height = node.measured?.height ?? DEFAULT_NODE_HEIGHT;
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
      setContextMenu({ x: event.clientX, y: event.clientY, target: { kind: 'node', nodeId: node.id } });
    },
    []
  );

  const onPaneContextMenu = useCallback(
    (event: React.MouseEvent | MouseEvent) => {
      event.preventDefault();
      const position = screenToFlowPosition({ x: event.clientX, y: event.clientY });
      setContextMenu({ x: event.clientX, y: event.clientY, target: { kind: 'pane', position } });
    },
    [screenToFlowPosition]
  );

  // Files dropped from the OS (Tauri reports absolute paths). Each must resolve
  // inside the Vault; the backend refuses everything else with a message.
  const linkDroppedFiles = useCallback(
    async (absolutePaths: string[], point: { x: number; y: number }) => {
      const rect = containerRef.current?.getBoundingClientRect();
      if (!rect || point.x < rect.left || point.x > rect.right || point.y < rect.top || point.y > rect.bottom) {
        return; // Dropped on the side panel or toolbar, not the Graph
      }
      const origin = screenToFlowPosition(point);
      const transport = getBackendTransport();
      const { linkVaultFile } = useGraphStore.getState();
      for (const [index, absolutePath] of absolutePaths.entries()) {
        try {
          const path = await transport.resolveDroppedFile(absolutePath);
          await linkVaultFile(path, {
            x: origin.x + index * DROP_CASCADE_PX,
            y: origin.y + index * DROP_CASCADE_PX,
          });
        } catch (error) {
          useUIStore.getState().setNotice(error instanceof Error ? error.message : String(error));
        }
      }
    },
    [screenToFlowPosition]
  );

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    // Loaded lazily: the module touches Tauri globals that only exist in the webview.
    void import('@tauri-apps/api/webview')
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type !== 'drop') return;
          // Tauri reports physical pixels; the DOM (and screenToFlowPosition) use CSS pixels.
          const scale = window.devicePixelRatio || 1;
          void linkDroppedFiles(event.payload.paths, {
            x: event.payload.position.x / scale,
            y: event.payload.position.y / scale,
          });
        })
      )
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch((error) => logger.warn('File drag-drop unavailable:', error));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [linkDroppedFiles]);

  // Files may change while the app is in the background: re-stat on every focus.
  useEffect(() => {
    const refresh = () => {
      void useGraphStore.getState().refreshAllFileNodeStats();
    };
    window.addEventListener('focus', refresh);
    return () => window.removeEventListener('focus', refresh);
  }, []);

  // The webview must not navigate to a dropped file when Tauri hands the
  // event through to the DOM.
  const preventNativeDrop = useCallback((event: React.DragEvent) => {
    event.preventDefault();
  }, []);

  // Keyboard shortcuts. The handler reads store state via getState() so it
  // stays stable and is attached exactly once, instead of detaching and
  // re-attaching on every node mutation (e.g. each streaming flush).
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const graph = useGraphStore.getState();
      const ui = useUIStore.getState();

      if (togglePreviewShortcut(e, graph, ui)) return;
      if (editShortcut(e, graph, ui)) return;
      replyShortcut(e, graph, ui);
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

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

        // Create edge from source to new node via store action
        onConnect({ source: connectingNodeId.current, target: newNodeId, sourceHandle: null, targetHandle: null });
      }

      connectingNodeId.current = null;
    },
    [screenToFlowPosition, createUserNode, onConnect]
  );

  // Wrap onConnect to handle edge direction when connecting to agent nodes
  // If user drags from a user node TO an agent node, reverse the direction
  // so the agent becomes the parent (source) and user node becomes child (target)
  const handleConnect = useCallback(
    (connection: Connection) => {
      const { nodeData } = useGraphStore.getState();
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
    [onConnect]
  );

  return (
    <div
      className="graph-container"
      ref={containerRef}
      onDragOver={preventNativeDrop}
      onDrop={preventNativeDrop}
    >
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
        onPaneContextMenu={onPaneContextMenu}
        nodeTypes={nodeTypes}
        fitView
        zoomOnDoubleClick={false}
        proOptions={{ hideAttribution: true }}
      >
        <Controls />
        {/* Same grammar as the icon: user = filled mint, assistant = mint outline, file = muted teal */}
        <MiniMap
          nodeColor={minimapColor}
          nodeStrokeColor={MINT}
          nodeStrokeWidth={4}
          maskColor="rgba(4, 37, 44, 0.75)"
        />
        <AlignmentGuides guides={alignmentGuides} />
      </ReactFlow>
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          target={contextMenu.target}
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
}
