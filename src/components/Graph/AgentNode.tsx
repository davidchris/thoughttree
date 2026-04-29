import { useMemo } from "react";
import {
  Handle,
  Position,
  NodeProps,
} from "@xyflow/react";
import { AgentNodeData, PROVIDER_SHORT_NAMES } from "../../types";
import { useGraphStore } from "../../store/useGraphStore";
import "./styles.css";

const SUMMARY_THRESHOLD = 100;

export function AgentNode({ id, selected }: NodeProps) {
  const nodeData = useGraphStore((state) => state.nodeData.get(id) as AgentNodeData | undefined);
  const content = nodeData?.content ?? '';
  const summary = nodeData?.summary;
  const provider = nodeData?.provider;

  // Subscribe directly to store for streaming state (fixes reactivity issue)
  const streamingNodeIds = useGraphStore((state) => state.streamingNodeIds);
  const isNodeBlocked = useGraphStore((state) => state.isNodeBlocked);
  const createUserNodeDownstream = useGraphStore(
    (state) => state.createUserNodeDownstream,
  );
  const togglePreviewNode = useGraphStore((state) => state.togglePreviewNode);
  const setPreviewNode = useGraphStore((state) => state.setPreviewNode);

  const isStreaming = streamingNodeIds.has(id);
  const isBlocked = isNodeBlocked(id);

  // Compute collapsed text: short content shown directly, long content uses AI summary
  const collapsedText = useMemo(() => {
    if (!content) return '';
    if (content.length <= SUMMARY_THRESHOLD) return content;
    if (summary) return summary;
    return content.slice(0, 30) + '...'; // Fallback while loading
  }, [content, summary]);

  const hasMore = content.length > SUMMARY_THRESHOLD || content.length > 30;
  const isGeneratingSummary = content.length > SUMMARY_THRESHOLD && !summary && !isStreaming;

  const handleContinue = () => {
    if (isBlocked) return;
    createUserNodeDownstream(id);
  };

  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setPreviewNode(id);
  };

  const handleToggleExpand = (e: React.MouseEvent) => {
    e.stopPropagation();
    togglePreviewNode(id);
  };

  return (
    <div
      className={`thought-node agent-node ${selected ? "selected" : ""} ${isStreaming ? "streaming" : ""}`}
      onDoubleClick={handleDoubleClick}
    >
      <Handle type="target" position={Position.Top} />

      <div className="node-header">
        <span className="node-role">
          {provider ? PROVIDER_SHORT_NAMES[provider] : 'Assistant'}
        </span>
        {(hasMore || isStreaming) && (
          <button
            className="expand-toggle"
            onClick={handleToggleExpand}
            title="Preview in side panel (P)"
          >
            ▼
          </button>
        )}
        {isStreaming && <span className="streaming-badge">Generating...</span>}
      </div>

      <div className="node-content">
        {content ? (
          <>
            {collapsedText}
            {isGeneratingSummary && <span className="summary-loading"> ⋯</span>}
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
          disabled={isBlocked}
          title="Continue conversation"
        >
          Continue
        </button>
      )}

      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}
