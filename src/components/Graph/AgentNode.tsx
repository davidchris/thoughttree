import {
  Handle,
  Position,
  NodeProps,
  NodeResizer,
} from "@xyflow/react";
import { AgentFlowNodeData } from "../../types";
import { useGraphStore } from "../../store/useGraphStore";
import "./styles.css";

const COLLAPSED_PREVIEW_LENGTH = 30;

type AgentNodeProps = NodeProps & {
  data: AgentFlowNodeData;
};

export function AgentNode({ id, data, selected }: AgentNodeProps) {
  const { nodeData } = data;
  const content = nodeData.content;

  // Subscribe directly to store for streaming state (fixes reactivity issue)
  const streamingNodeId = useGraphStore((state) => state.streamingNodeId);
  const createUserNodeDownstream = useGraphStore(
    (state) => state.createUserNodeDownstream,
  );
  const togglePreviewNode = useGraphStore((state) => state.togglePreviewNode);

  const isStreaming = streamingNodeId === id;
  const isAnyStreaming = streamingNodeId !== null;

  const preview = content.slice(0, COLLAPSED_PREVIEW_LENGTH);
  const hasMore = content.length > COLLAPSED_PREVIEW_LENGTH;

  const handleContinue = () => {
    if (isAnyStreaming) return;
    createUserNodeDownstream(id);
  };

  const handleToggleExpand = (e: React.MouseEvent) => {
    e.stopPropagation();
    togglePreviewNode(id);
  };

  return (
    <div
      className={`thought-node agent-node ${selected ? "selected" : ""} ${isStreaming ? "streaming" : ""} collapsed`}
    >
      <NodeResizer
        minWidth={150}
        minHeight={60}
        isVisible={selected}
        handleClassName="node-resize-handle"
      />
      <Handle type="target" position={Position.Top} />

      <div className="node-header">
        <span className="node-role">Assistant</span>
        {hasMore && !isStreaming && (
          <button
            className="expand-toggle"
            onClick={handleToggleExpand}
            title="Preview in side panel (Space)"
          >
            ▼
          </button>
        )}
        {isStreaming && <span className="streaming-badge">Generating...</span>}
      </div>

      <div className="node-content">
        {content ? (
          <>
            {preview}
            {hasMore && "..."}
          </>
        ) : (
          <span className="node-placeholder">
            {isStreaming ? "Waiting for response..." : "Empty response"}
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
