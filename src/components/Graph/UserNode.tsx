import { useState, useRef, useEffect } from 'react';
import { Handle, Position, NodeProps } from '@xyflow/react';
import { UserFlowNodeData } from '../../types';
import { useGraphStore } from '../../store/useGraphStore';
import { sendPrompt } from '../../lib/tauri';
import './styles.css';

type UserNodeProps = NodeProps & {
  data: UserFlowNodeData;
};

export function UserNode({ id, data, selected }: UserNodeProps) {
  const { nodeData } = data;
  const content = nodeData.content;
  const [isExpanded, setIsExpanded] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const {
    editingNodeId,
    streamingNodeId,
    updateNodeContent,
    setEditing,
    createAgentNodeDownstream,
    buildConversationContext,
    appendToNode,
    setStreaming,
  } = useGraphStore();

  const isEditing = editingNodeId === id;
  const isAnyStreaming = streamingNodeId !== null;
  const preview = content.slice(0, 100);
  const hasMore = content.length > 100;

  // Auto-focus textarea when entering edit mode
  useEffect(() => {
    if (isEditing && textareaRef.current) {
      textareaRef.current.focus();
      textareaRef.current.selectionStart = textareaRef.current.value.length;
    }
  }, [isEditing]);

  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setEditing(id);
  };

  const handleContentChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    updateNodeContent(id, e.target.value);
  };

  const handleBlur = () => {
    // Only exit editing if there's content
    if (content.trim()) {
      setEditing(null);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    // Cmd/Ctrl + Enter to submit
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      handleGenerate();
    }
    // Escape to exit editing
    if (e.key === 'Escape') {
      setEditing(null);
    }
  };

  const handleGenerate = async () => {
    if (!content.trim() || isAnyStreaming) return;

    console.log('[Generate] Starting generation...');

    // Exit edit mode first
    setEditing(null);

    // Create downstream agent node
    const agentNodeId = createAgentNodeDownstream(id);
    console.log('[Generate] Created agent node:', agentNodeId);

    // Build context by traversing parents (including this node)
    const context = buildConversationContext(id);
    console.log('[Generate] Context:', context);

    try {
      console.log('[Generate] Calling sendPrompt...');
      await sendPrompt(
        agentNodeId,
        context,
        (chunk) => appendToNode(agentNodeId, chunk)
      );
      console.log('[Generate] sendPrompt completed successfully');
    } catch (error) {
      console.error('[Generate] Generation failed:', error);
      appendToNode(agentNodeId, `\n\n[Error: ${error}]`);
    } finally {
      console.log('[Generate] Finally block - calling setStreaming(null)');
      setStreaming(null);
      console.log('[Generate] setStreaming(null) called');
    }
  };

  const handleExpandClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsExpanded(!isExpanded);
  };

  return (
    <div
      className={`thought-node user-node ${selected ? 'selected' : ''} ${isEditing ? 'editing' : ''}`}
      onDoubleClick={handleDoubleClick}
    >
      <Handle type="target" position={Position.Top} />

      <div className="node-header">
        <span className="node-role">User</span>
      </div>

      {isEditing ? (
        <textarea
          ref={textareaRef}
          className="node-textarea"
          value={content}
          onChange={handleContentChange}
          onBlur={handleBlur}
          onKeyDown={handleKeyDown}
          placeholder="Enter your message..."
        />
      ) : (
        <div
          className={`node-content ${isExpanded ? 'expanded' : ''}`}
          onClick={hasMore ? handleExpandClick : undefined}
        >
          {isExpanded ? content : preview}
          {hasMore && !isExpanded && '...'}
        </div>
      )}

      {!isEditing && content.trim() && (
        <button
          className="generate-button"
          onClick={handleGenerate}
          disabled={isAnyStreaming}
          title="Generate response (Cmd+Enter)"
        >
          {isAnyStreaming ? '...' : 'Generate'}
        </button>
      )}

      {!content.trim() && !isEditing && (
        <div className="node-placeholder">Double-click to edit</div>
      )}

      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}
