import { useState } from 'react';
import { Handle, Position, NodeProps } from '@xyflow/react';
import { AgentFlowNodeData } from '../../types';
import { useGraphStore } from '../../store/useGraphStore';
import './styles.css';

type AgentNodeProps = NodeProps & {
  data: AgentFlowNodeData;
};

export function AgentNode({ id, data, selected }: AgentNodeProps) {
  const { nodeData } = data;
  const content = nodeData.content;
  const [isExpanded, setIsExpanded] = useState(false);

  // Subscribe directly to store for streaming state (fixes reactivity issue)
  const streamingNodeId = useGraphStore((state) => state.streamingNodeId);
  const createUserNodeDownstream = useGraphStore((state) => state.createUserNodeDownstream);

  const isStreaming = streamingNodeId === id;
  const isAnyStreaming = streamingNodeId !== null;

  // Debug logging
  console.log(`[AgentNode ${id.slice(0,8)}] streamingNodeId=${streamingNodeId?.slice(0,8) ?? 'null'}, isStreaming=${isStreaming}`);
  const preview = content.slice(0, 100);
  const hasMore = content.length > 100;

  const handleContinue = () => {
    if (isAnyStreaming) return;
    createUserNodeDownstream(id);
  };

  const handleExpandClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsExpanded(!isExpanded);
  };

  return (
    <div
      className={`thought-node agent-node ${selected ? 'selected' : ''} ${isStreaming ? 'streaming' : ''}`}
    >
      <Handle type="target" position={Position.Top} />

      <div className="node-header">
        <span className="node-role">Assistant</span>
        {isStreaming && <span className="streaming-badge">Generating...</span>}
      </div>

      <div
        className={`node-content ${isExpanded ? 'expanded' : ''}`}
        onClick={hasMore ? handleExpandClick : undefined}
      >
        {content ? (
          <>
            {isExpanded ? content : preview}
            {hasMore && !isExpanded && '...'}
          </>
        ) : (
          <span className="node-placeholder">
            {isStreaming ? 'Waiting for response...' : 'Empty response'}
          </span>
        )}
      </div>

      {!isStreaming && content.trim() && (
        <button
          className="continue-button"
          onClick={handleContinue}
          disabled={isAnyStreaming}
          title="Continue conversation"
        >
          Continue
        </button>
      )}

      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}
