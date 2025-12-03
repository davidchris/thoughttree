import { useCallback, useRef } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  NodeTypes,
  useReactFlow,
  addEdge,
  type OnConnectEnd,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { UserNode } from './UserNode';
import { AgentNode } from './AgentNode';
import { useGraphStore } from '../../store/useGraphStore';
import './styles.css';

const nodeTypes: NodeTypes = {
  user: UserNode,
  agent: AgentNode,
};

export function Graph() {
  const { nodes, edges, onNodesChange, onEdgesChange, onConnect, selectNode, createUserNode, setEditing } = useGraphStore();
  const { screenToFlowPosition } = useReactFlow();
  const connectingNodeId = useRef<string | null>(null);

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

  return (
    <div className="graph-container">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onConnectStart={onConnectStart}
        onConnectEnd={onConnectEnd}
        onNodeClick={onNodeClick}
        onPaneClick={onPaneClick}
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
    </div>
  );
}
