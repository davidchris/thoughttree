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
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { UserNode } from './UserNode';
import { AgentNode } from './AgentNode';
import { ContextMenu } from './ContextMenu';
import { useGraphStore } from '../../store/useGraphStore';
import './styles.css';

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
    streamingNodeId,
    editingNodeId,
  } = useGraphStore();
  const { screenToFlowPosition } = useReactFlow();
  const connectingNodeId = useRef<string | null>(null);

  // Context menu state
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    nodeId: string;
  } | null>(null);

  const onNodeContextMenu = useCallback(
    (event: React.MouseEvent, node: { id: string }) => {
      event.preventDefault();
      setContextMenu({ x: event.clientX, y: event.clientY, nodeId: node.id });
    },
    []
  );

  // Keyboard shortcut: Enter to reply to selected agent node
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't trigger if editing a node or streaming
      if (editingNodeId || streamingNodeId) return;

      // Check if selected node is an agent node
      if (!selectedNodeId) return;
      const data = nodeData.get(selectedNodeId);
      if (!data || data.role !== 'assistant') return;

      // Enter to reply
      if (e.key === 'Enter') {
        e.preventDefault();
        createUserNodeDownstream(selectedNodeId);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [selectedNodeId, nodeData, createUserNodeDownstream, streamingNodeId, editingNodeId]);

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
        // Single-click: deselect and exit editing
        selectNode(null);
        setEditing(null);
      }
    },
    [screenToFlowPosition, createUserNode, selectNode, setEditing]
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
        onNodesChange={onNodesChange}
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
