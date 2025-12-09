import { useState, useEffect } from "react";
import {
  Handle,
  Position,
  NodeProps,
  NodeResizer,
  useUpdateNodeInternals,
} from "@xyflow/react";
import { AgentFlowNodeData } from "../../types";
import { useGraphStore } from "../../store/useGraphStore";
import { MarkdownContent } from "./MarkdownContent";
import "./styles.css";

const COLLAPSED_PREVIEW_LENGTH = 30;

type AgentNodeProps = NodeProps & {
  data: AgentFlowNodeData;
};

export function AgentNode({ id, data, selected }: AgentNodeProps) {
  const { nodeData } = data;
  const content = nodeData.content;
  const [isExpanded, setIsExpanded] = useState(false);
  const updateNodeInternals = useUpdateNodeInternals();

  // Notify ReactFlow when node dimensions change due to expansion
  useEffect(() => {
    updateNodeInternals(id);
  }, [isExpanded, id, updateNodeInternals]);

  // Subscribe directly to store for streaming state (fixes reactivity issue)
  const streamingNodeId = useGraphStore((state) => state.streamingNodeId);
  const createUserNodeDownstream = useGraphStore(
    (state) => state.createUserNodeDownstream,
  );

  const isStreaming = streamingNodeId === id;
  const isAnyStreaming = streamingNodeId !== null;

  // Debug logging
  console.log(
    `[AgentNode ${id.slice(0, 8)}] streamingNodeId=${streamingNodeId?.slice(0, 8) ?? "null"}, isStreaming=${isStreaming}`,
  );
  const isCollapsed = !isExpanded && !isStreaming;
  const preview = content.slice(
    0,
    isCollapsed ? COLLAPSED_PREVIEW_LENGTH : 100,
  );
  const hasMore = content.length > COLLAPSED_PREVIEW_LENGTH;

  const handleContinue = () => {
    if (isAnyStreaming) return;
    createUserNodeDownstream(id);
  };

  const handleToggleExpand = (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsExpanded(!isExpanded);
  };

  return (
    <div
      className={`thought-node agent-node ${selected ? "selected" : ""} ${isStreaming ? "streaming" : ""} ${isCollapsed ? "collapsed" : ""}`}
    >
      <NodeResizer
        minWidth={isCollapsed ? 150 : 220}
        minHeight={isCollapsed ? 60 : 100}
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
            title={isExpanded ? "Collapse" : "Expand"}
          >
            {isExpanded ? "▲" : "▼"}
          </button>
        )}
        {isStreaming && <span className="streaming-badge">Generating...</span>}
      </div>

      <div className={`node-content ${isExpanded ? "expanded" : ""}`}>
        {content ? (
          isExpanded ? (
            <MarkdownContent content={content} />
          ) : (
            <>
              {preview}
              {hasMore && "..."}
            </>
          )
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
