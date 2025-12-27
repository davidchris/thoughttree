import { useState, useRef, useEffect, useMemo } from "react";
import {
  Handle,
  Position,
  NodeProps,
  NodeResizer,
} from "@xyflow/react";
import { UserFlowNodeData } from "../../types";
import { useGraphStore } from "../../store/useGraphStore";
import { sendPrompt } from "../../lib/tauri";
import { FileAutocomplete, FileAutocompleteRef } from "../FileAutocomplete";
import "./styles.css";

const SUMMARY_THRESHOLD = 100;

interface AutocompleteState {
  isOpen: boolean;
  query: string;
  position: { top: number; left: number };
  triggerIndex: number;
}

type UserNodeProps = NodeProps & {
  data: UserFlowNodeData;
};

export function UserNode({ id, data, selected }: UserNodeProps) {
  const { nodeData } = data;
  const content = nodeData.content;
  const [autocomplete, setAutocomplete] = useState<AutocompleteState | null>(
    null,
  );
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const autocompleteRef = useRef<FileAutocompleteRef>(null);

  const {
    editingNodeId,
    streamingNodeId,
    updateNodeContent,
    setEditing,
    createAgentNodeDownstream,
    buildConversationContext,
    appendToNode,
    setStreaming,
    togglePreviewNode,
    setPreviewNode,
    triggerSidePanelEditMode,
  } = useGraphStore();

  const isEditing = editingNodeId === id;
  const isAnyStreaming = streamingNodeId !== null;

  // Compute collapsed text: short content shown directly, long content uses AI summary
  const collapsedText = useMemo(() => {
    if (!content) return '';
    if (content.length <= SUMMARY_THRESHOLD) return content;
    if (nodeData.summary) return nodeData.summary;
    return content.slice(0, 30) + '...'; // Fallback while loading
  }, [content, nodeData.summary]);

  const hasMore = content.length > SUMMARY_THRESHOLD || content.length > 30;
  const isGeneratingSummary = content.length > SUMMARY_THRESHOLD && !nodeData.summary;

  // Auto-focus textarea when entering edit mode
  useEffect(() => {
    if (isEditing && textareaRef.current) {
      textareaRef.current.focus();
      textareaRef.current.selectionStart = textareaRef.current.value.length;
    }
  }, [isEditing]);

  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setPreviewNode(id);
    triggerSidePanelEditMode();
  };

  const handleContentChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newValue = e.target.value;
    const cursorPos = e.target.selectionStart;

    updateNodeContent(id, newValue);

    // Check for @ trigger
    const textBeforeCursor = newValue.slice(0, cursorPos);
    const atIndex = textBeforeCursor.lastIndexOf("@");

    console.log(
      "[Autocomplete] textBeforeCursor:",
      textBeforeCursor,
      "atIndex:",
      atIndex,
    );

    if (atIndex !== -1) {
      // Check if @ is at start or preceded by whitespace
      const charBefore = atIndex > 0 ? textBeforeCursor[atIndex - 1] : " ";
      if (/\s/.test(charBefore) || atIndex === 0) {
        const query = textBeforeCursor.slice(atIndex + 1);

        console.log("[Autocomplete] Valid @ found, query:", query);

        // Only show if query doesn't contain whitespace (still typing the mention)
        if (!/\s/.test(query)) {
          const textarea = e.target;
          const rect = textarea.getBoundingClientRect();
          const position = { top: rect.bottom + 4, left: rect.left };
          console.log(
            "[Autocomplete] Setting autocomplete state, position:",
            position,
          );
          setAutocomplete({
            isOpen: true,
            query,
            position,
            triggerIndex: atIndex,
          });
          return;
        }
      }
    }

    // Close autocomplete if no valid trigger
    if (autocomplete?.isOpen) {
      setAutocomplete(null);
    }
  };

  const handleBlur = (e: React.FocusEvent) => {
    // Check if focus is moving to autocomplete
    const relatedTarget = e.relatedTarget as HTMLElement;
    if (relatedTarget?.closest(".file-autocomplete")) {
      return; // Don't close editing
    }

    // Close autocomplete on blur
    setAutocomplete(null);

    // Only exit editing if there's content
    if (content.trim()) {
      setEditing(null);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    // Let autocomplete handle keys first if open
    if (autocomplete?.isOpen && autocompleteRef.current) {
      const handled = autocompleteRef.current.handleKeyDown(e.nativeEvent);
      if (handled) {
        e.preventDefault();
        return;
      }
    }

    // Cmd/Ctrl + Enter to submit
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      handleGenerate();
    }
    // Escape to exit editing (only if autocomplete not open)
    if (e.key === "Escape" && !autocomplete?.isOpen) {
      setEditing(null);
    }
  };

  const handleGenerate = async () => {
    if (!content.trim() || isAnyStreaming) return;

    console.log("[Generate] Starting generation...");

    // Exit edit mode first
    setEditing(null);

    // Create downstream agent node
    const agentNodeId = createAgentNodeDownstream(id);
    console.log("[Generate] Created agent node:", agentNodeId);

    // Build context by traversing parents (including this node)
    const context = buildConversationContext(id);
    console.log("[Generate] Context:", context);

    try {
      console.log("[Generate] Calling sendPrompt...");
      await sendPrompt(agentNodeId, context, (chunk) =>
        appendToNode(agentNodeId, chunk),
      );
      console.log("[Generate] sendPrompt completed successfully");
    } catch (error) {
      console.error("[Generate] Generation failed:", error);
      appendToNode(agentNodeId, `\n\n[Error: ${error}]`);
    } finally {
      console.log("[Generate] Finally block - calling setStreaming(null)");
      setStreaming(null);
      console.log("[Generate] setStreaming(null) called");
    }
  };

  const handleToggleExpand = (e: React.MouseEvent) => {
    e.stopPropagation();
    togglePreviewNode(id);
  };

  const handleFileSelect = (filePath: string) => {
    if (!autocomplete || !textareaRef.current) return;

    const textarea = textareaRef.current;
    const beforeAt = content.slice(0, autocomplete.triggerIndex);
    const afterCursor = content.slice(textarea.selectionStart);

    // Format: @/relative/path/to/file.md
    const mention = `@/${filePath}`;
    const newContent = beforeAt + mention + afterCursor;

    updateNodeContent(id, newContent);
    setAutocomplete(null);

    // Position cursor after the mention
    const newCursorPos = autocomplete.triggerIndex + mention.length;
    setTimeout(() => {
      textarea.selectionStart = newCursorPos;
      textarea.selectionEnd = newCursorPos;
      textarea.focus();
    }, 0);
  };

  return (
    <div
      className={`thought-node user-node ${selected ? "selected" : ""} ${isEditing ? "editing" : ""} ${!isEditing ? "collapsed" : ""}`}
      onDoubleClick={handleDoubleClick}
    >
      <NodeResizer
        minWidth={isEditing ? 220 : 120}
        minHeight={isEditing ? 100 : 120}
        isVisible={selected}
        handleClassName="node-resize-handle"
      />
      <Handle type="target" position={Position.Top} />

      <div className="node-header">
        <span className="node-role">User</span>
        {hasMore && !isEditing && (
          <button
            className="expand-toggle"
            onClick={handleToggleExpand}
            title="Preview in side panel (P)"
          >
            ▼
          </button>
        )}
      </div>

      {isEditing ? (
        <>
          <textarea
            ref={textareaRef}
            className="node-textarea"
            value={content}
            onChange={handleContentChange}
            onBlur={handleBlur}
            onKeyDown={handleKeyDown}
            placeholder="Enter your message... (@ to mention files)"
          />
          {console.log("[UserNode] autocomplete state:", autocomplete)}
          {autocomplete?.isOpen && (
            <FileAutocomplete
              ref={autocompleteRef}
              isOpen={autocomplete.isOpen}
              query={autocomplete.query}
              position={autocomplete.position}
              onSelect={handleFileSelect}
              onClose={() => setAutocomplete(null)}
            />
          )}
        </>
      ) : (
        <div className="node-content">
          {content ? (
            <>
              {collapsedText}
              {isGeneratingSummary && <span className="summary-loading"> ⋯</span>}
            </>
          ) : null}
        </div>
      )}

      {!isEditing && content.trim() && (
        <button
          className="generate-button"
          onClick={handleGenerate}
          disabled={isAnyStreaming}
          title="Generate response (Cmd+Enter)"
        >
          {isAnyStreaming ? "..." : "Generate"}
        </button>
      )}

      {!content.trim() && !isEditing && (
        <div className="node-placeholder">Double-click to edit in panel</div>
      )}

      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}
