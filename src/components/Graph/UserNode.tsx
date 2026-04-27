import { useState, useRef, useEffect, useMemo, useCallback } from "react";
import {
  Handle,
  Position,
  NodeProps,
} from "@xyflow/react";
import { ImageAttachment, UserNodeData } from "../../types";
import { useGraphStore } from "../../store/useGraphStore";
import { resizeIfNeeded, fileToBase64 } from "../../lib/imageUtils";
import { FileAutocomplete, FileAutocompleteRef } from "../FileAutocomplete";
import { getCaretCoordinates } from "../../lib/caretCoordinates";
import { useNodeGeneration } from "../../hooks/useNodeGeneration";
import { logger } from "../../lib/logger";
import "./styles.css";

const SUMMARY_THRESHOLD = 100;

interface AutocompleteState {
  isOpen: boolean;
  query: string;
  position: { top: number; left: number };
  triggerIndex: number;
}

export function UserNode({ id, selected }: NodeProps) {
  const nodeData = useGraphStore((state) => state.nodeData.get(id) as UserNodeData | undefined);
  const content = nodeData?.content ?? '';
  const images = nodeData?.images ?? [];
  const [autocomplete, setAutocomplete] = useState<AutocompleteState | null>(
    null,
  );
  const [isDragOver, setIsDragOver] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const nodeRef = useRef<HTMLDivElement>(null);
  const autocompleteRef = useRef<FileAutocompleteRef>(null);

  const editingNodeId = useGraphStore((state) => state.editingNodeId);
  const updateNodeContent = useGraphStore((state) => state.updateNodeContent);
  const setEditing = useGraphStore((state) => state.setEditing);
  const isNodeBlocked = useGraphStore((state) => state.isNodeBlocked);
  const togglePreviewNode = useGraphStore((state) => state.togglePreviewNode);
  const setPreviewNode = useGraphStore((state) => state.setPreviewNode);
  const triggerSidePanelEditMode = useGraphStore((state) => state.triggerSidePanelEditMode);
  const addNodeImage = useGraphStore((state) => state.addNodeImage);
  const removeNodeImage = useGraphStore((state) => state.removeNodeImage);
  const generateNode = useNodeGeneration();

  const isEditing = editingNodeId === id;
  const isBlocked = isNodeBlocked(id);

  // Compute collapsed text: short content shown directly, long content uses AI summary
  const summary = nodeData?.summary;
  const collapsedText = useMemo(() => {
    if (!content) return '';
    if (content.length <= SUMMARY_THRESHOLD) return content;
    if (summary) return summary;
    return content.slice(0, 30) + '...'; // Fallback while loading
  }, [content, summary]);

  const hasMore = content.length > SUMMARY_THRESHOLD || content.length > 30;
  const isGeneratingSummary = content.length > SUMMARY_THRESHOLD && !summary;

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

    if (atIndex !== -1) {
      // Check if @ is at start or preceded by whitespace
      const charBefore = atIndex > 0 ? textBeforeCursor[atIndex - 1] : " ";
      if (/\s/.test(charBefore) || atIndex === 0) {
        const query = textBeforeCursor.slice(atIndex + 1);

        // Only show if query doesn't contain whitespace (still typing the mention)
        if (!/\s/.test(query)) {
          const textarea = e.target;
          const caret = getCaretCoordinates(textarea, textarea.selectionStart);
          const position = { top: caret.top + caret.height + 4, left: caret.left };
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
    if (!content.trim() || isBlocked) return;

    // Exit edit mode first
    setEditing(null);
    await generateNode({ userNodeId: id });
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

  // Process and add an image file
  const processImageFile = useCallback(async (file: File) => {
    if (!file.type.startsWith('image/')) return;

    try {
      const resized = await resizeIfNeeded(file);
      const base64 = await fileToBase64(resized);
      const image: ImageAttachment = {
        data: base64,
        mimeType: file.type,
        name: file.name,
      };
      addNodeImage(id, image);
    } catch (error) {
      logger.error('Failed to process image:', error);
    }
  }, [id, addNodeImage]);

  // Handle paste events (for Cmd+V with images)
  const handlePaste = useCallback(async (e: React.ClipboardEvent) => {
    const items = e.clipboardData?.items;
    if (!items) return;

    for (const item of Array.from(items)) {
      if (item.type.startsWith('image/')) {
        e.preventDefault();
        const file = item.getAsFile();
        if (file) {
          await processImageFile(file);
        }
      }
    }
  }, [processImageFile]);

  // Handle drag over
  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer.types.includes('Files')) {
      setIsDragOver(true);
    }
  }, []);

  // Handle drag leave
  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);
  }, []);

  // Handle drop events
  const handleDrop = useCallback(async (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);

    const files = e.dataTransfer.files;
    for (const file of Array.from(files)) {
      if (file.type.startsWith('image/')) {
        await processImageFile(file);
      }
    }
  }, [processImageFile]);

  // Handle image removal
  const handleRemoveImage = useCallback((index: number, e: React.MouseEvent) => {
    e.stopPropagation();
    removeNodeImage(id, index);
  }, [id, removeNodeImage]);

  return (
    <div
      ref={nodeRef}
      className={`thought-node user-node ${selected ? "selected" : ""} ${isDragOver ? "drag-over" : ""}`}
      onDoubleClick={handleDoubleClick}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <Handle type="target" position={Position.Top} />

      <div className="node-header">
        <span className="node-role">User</span>
        {images.length > 0 && !isEditing && (
          <span className="image-count" title={`${images.length} image${images.length > 1 ? 's' : ''} attached`}>
            {images.length}
          </span>
        )}
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

      {/* Image thumbnails */}
      {images.length > 0 && (
        <div className="image-thumbnails">
          {images.map((img, index) => (
            <div key={index} className="image-thumbnail">
              <img
                src={`data:${img.mimeType};base64,${img.data}`}
                alt={img.name || `Image ${index + 1}`}
              />
              <button
                className="image-remove"
                onClick={(e) => handleRemoveImage(index, e)}
                title="Remove image"
              >
                ×
              </button>
            </div>
          ))}
        </div>
      )}

      {isEditing ? (
        <>
          <textarea
            ref={textareaRef}
            className="node-textarea"
            value={content}
            onChange={handleContentChange}
            onBlur={handleBlur}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            placeholder="Enter your message... (@ to mention files, paste or drop images)"
          />
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

      {!isEditing && (content.trim() || images.length > 0) && (
        <button
          className="generate-button"
          onClick={handleGenerate}
          disabled={isBlocked}
          title="Generate response (Cmd+Enter)"
        >
          {isBlocked ? "..." : "Generate"}
        </button>
      )}

      {!content.trim() && images.length === 0 && !isEditing && (
        <div className="node-placeholder">Double-click to edit in panel</div>
      )}

      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}
