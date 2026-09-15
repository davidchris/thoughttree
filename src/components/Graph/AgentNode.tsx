import { useMemo } from "react";
import {
  Handle,
  Position,
  NodeProps,
} from "@xyflow/react";
import { AgentNodeData, providerShortName } from "../../types";
import { useGraphStore } from "../../store/useGraphStore";
import { useUIStore } from "../../store/useUIStore";
import "./styles.css";

const SUMMARY_THRESHOLD = 100;

interface NodeStateFlags {
  selected?: boolean;
  isStreaming: boolean;
  isBlocked: boolean;
  isFlashing: boolean;
}

function agentNodeClassName({ selected, isStreaming, isBlocked, isFlashing }: NodeStateFlags) {
  return [
    "thought-node",
    "agent-node",
    selected && "selected",
    isStreaming && "streaming",
    isBlocked && !isStreaming && "blocked",
    isFlashing && "flash",
  ]
    .filter(Boolean)
    .join(" ");
}

interface AgentNodeBodyProps {
  collapsedText: string;
  hasContent: boolean;
  isGeneratingSummary: boolean;
  isStreaming: boolean;
}

function AgentNodeBody({
  collapsedText,
  hasContent,
  isGeneratingSummary,
  isStreaming,
}: AgentNodeBodyProps) {
  if (!hasContent) {
    return (
      <span className="node-placeholder">
        {isStreaming ? "Waiting for response..." : "Empty response"}
      </span>
    );
  }

  return (
    <>
      {collapsedText}
      {isGeneratingSummary && <span className="summary-loading"> ⋯</span>}
    </>
  );
}

export function AgentNode({ id, selected }: NodeProps) {
  const nodeData = useGraphStore((state) => state.nodeData.get(id) as AgentNodeData | undefined);
  const content = nodeData?.content ?? '';
  const summary = nodeData?.summary;
  const provider = nodeData?.provider;

  // Subscribe directly to store for streaming state (fixes reactivity issue)
  const activeTurns = useGraphStore((state) => state.activeTurns);
  const isNodeBlocked = useGraphStore((state) => state.isNodeBlocked);
  const createUserNodeDownstream = useGraphStore(
    (state) => state.createUserNodeDownstream,
  );
  const togglePreviewNode = useUIStore((state) => state.togglePreviewNode);
  const setPreviewNode = useUIStore((state) => state.setPreviewNode);
  const isFlashing = useUIStore((state) => state.flashNodeId === id);

  const isStreaming = activeTurns.has(id);
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
      className={agentNodeClassName({ selected, isStreaming, isBlocked, isFlashing })}
      onDoubleClick={handleDoubleClick}
    >
      <Handle type="target" position={Position.Top} />

      <div className="node-header">
        <span className="node-role">
          {providerShortName(provider)}
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
        <AgentNodeBody
          collapsedText={collapsedText}
          hasContent={Boolean(content)}
          isGeneratingSummary={isGeneratingSummary}
          isStreaming={isStreaming}
        />
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
