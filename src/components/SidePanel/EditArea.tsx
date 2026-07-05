import { useEffect, useRef, useState } from 'react';
import { useGraphStore } from '../../store/useGraphStore';
import { FileAutocomplete, FileAutocompleteRef } from '../FileAutocomplete';
import { getCaretCoordinates } from '../../lib/caretCoordinates';
import type { ImageAttachment } from '../../types';
import { useImageAttachments } from './useImageAttachments';

interface AutocompleteState {
  isOpen: boolean;
  query: string;
  position: { top: number; left: number };
  triggerIndex: number;
}

interface EditAreaProps {
  nodeId: string;
  initialContent: string;
  images: ImageAttachment[];
  onGenerate: () => void;
}

/**
 * Textarea editor for a user node: live store updates, @-file autocomplete,
 * and image attachment via paste or drag-and-drop.
 */
export function EditArea({ nodeId, initialContent, images, onGenerate }: EditAreaProps) {
  const updateNodeContent = useGraphStore((state) => state.updateNodeContent);
  const [editContent, setEditContent] = useState(initialContent);
  const [autocomplete, setAutocomplete] = useState<AutocompleteState | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const autocompleteRef = useRef<FileAutocompleteRef>(null);
  const {
    isDragOver,
    handlePaste,
    handleDragOver,
    handleDragLeave,
    handleDrop,
    handleRemoveImage,
  } = useImageAttachments(nodeId);

  // Focus the textarea once on mount (entering edit mode)
  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.focus();
      textareaRef.current.selectionStart = textareaRef.current.value.length;
    }
  }, []);

  const handleContentChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newValue = e.target.value;
    const cursorPos = e.target.selectionStart;

    setEditContent(newValue);
    updateNodeContent(nodeId, newValue);

    // Check for @ trigger
    const textBeforeCursor = newValue.slice(0, cursorPos);
    const atIndex = textBeforeCursor.lastIndexOf('@');

    if (atIndex !== -1) {
      // Check if @ is at start or preceded by whitespace
      const charBefore = atIndex > 0 ? textBeforeCursor[atIndex - 1] : ' ';
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

  const handleTextareaBlur = (e: React.FocusEvent) => {
    // Check if focus is moving to autocomplete
    const relatedTarget = e.relatedTarget as HTMLElement;
    if (relatedTarget?.closest('.file-autocomplete')) {
      return; // Don't close autocomplete
    }
    setAutocomplete(null);
  };

  const handleTextareaKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Escape closes the autocomplete; stop propagation so the panel's
    // window-level Escape handler doesn't also exit edit mode
    if (e.key === 'Escape' && autocomplete?.isOpen) {
      e.preventDefault();
      e.stopPropagation();
      setAutocomplete(null);
      return;
    }

    // Let autocomplete handle keys first if open
    if (autocomplete?.isOpen && autocompleteRef.current) {
      const handled = autocompleteRef.current.handleKeyDown(e.nativeEvent);
      if (handled) {
        e.preventDefault();
        return;
      }
    }

    // Cmd/Ctrl + Enter to generate
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      onGenerate();
    }
  };

  const handleFileSelect = (filePath: string) => {
    if (!autocomplete || !textareaRef.current) return;

    const textarea = textareaRef.current;
    const beforeAt = editContent.slice(0, autocomplete.triggerIndex);
    const afterCursor = editContent.slice(textarea.selectionStart);

    // Format: @/relative/path/to/file.md
    const mention = `@/${filePath}`;
    const newContent = beforeAt + mention + afterCursor;

    setEditContent(newContent);
    updateNodeContent(nodeId, newContent);
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
      className={`side-panel-edit-area ${isDragOver ? 'drag-over' : ''}`}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <textarea
        ref={textareaRef}
        className="side-panel-textarea"
        value={editContent}
        onChange={handleContentChange}
        onBlur={handleTextareaBlur}
        onKeyDown={handleTextareaKeyDown}
        onPaste={handlePaste}
        onDragOver={handleDragOver}
        onDrop={handleDrop}
        placeholder="Enter your message... (@ to mention files, paste or drop images)"
      />
      {images.length > 0 && (
        <div className="side-panel-image-thumbnails">
          {images.map((img, index) => (
            <div key={index} className="side-panel-image-thumbnail">
              <img
                src={`data:${img.mimeType};base64,${img.data}`}
                alt={img.name || `Image ${index + 1}`}
              />
              <button
                className="side-panel-image-remove"
                onClick={(e) => handleRemoveImage(index, e)}
                title="Remove image"
              >
                ×
              </button>
            </div>
          ))}
        </div>
      )}
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
    </div>
  );
}
