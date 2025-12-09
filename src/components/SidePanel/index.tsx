import { useEffect, useState, useRef } from 'react';
import { useGraphStore } from '../../store/useGraphStore';
import { MarkdownContent } from '../Graph/MarkdownContent';
import './styles.css';

const DEFAULT_WIDTH = 850; // ~100 character columns at 14px monospace
const MIN_WIDTH = 200;
const MAX_WIDTH_PERCENT = 0.8; // 80% of viewport

export function SidePanel() {
  const previewNodeId = useGraphStore((state) => state.previewNodeId);
  const nodeData = useGraphStore((state) => state.nodeData);
  const setPreviewNode = useGraphStore((state) => state.setPreviewNode);
  const updateNodeContent = useGraphStore((state) => state.updateNodeContent);

  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const [isResizing, setIsResizing] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const data = previewNodeId ? nodeData.get(previewNodeId) : null;
  const isUserNode = data?.role === 'user';

  // Reset edit state when node changes
  useEffect(() => {
    setIsEditing(false);
    if (data) {
      setEditContent(data.content);
    }
  }, [previewNodeId]);

  // Focus textarea when entering edit mode
  useEffect(() => {
    if (isEditing && textareaRef.current) {
      textareaRef.current.focus();
      textareaRef.current.selectionStart = textareaRef.current.value.length;
    }
  }, [isEditing]);

  // Handle Escape key
  useEffect(() => {
    if (!previewNodeId) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (isEditing) {
          setIsEditing(false);
        } else {
          setPreviewNode(null);
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [previewNodeId, setPreviewNode, isEditing]);

  const handleEdit = () => {
    if (data) {
      setEditContent(data.content);
      setIsEditing(true);
    }
  };

  const handleDone = () => {
    setIsEditing(false);
  };

  const handleContentChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newValue = e.target.value;
    setEditContent(newValue);
    if (previewNodeId) {
      updateNodeContent(previewNodeId, newValue);
    }
  };

  // Handle resize drag
  useEffect(() => {
    if (!isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      const maxWidth = window.innerWidth * MAX_WIDTH_PERCENT;
      const newWidth = Math.max(MIN_WIDTH, Math.min(maxWidth, window.innerWidth - e.clientX));
      setWidth(newWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
      document.body.classList.remove('resizing');
    };

    document.body.classList.add('resizing');
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizing]);

  const handleResizeStart = (e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
  };

  if (!previewNodeId || !data) {
    return null;
  }

  const isAgent = data.role === 'assistant';
  const formattedTime = new Date(data.timestamp).toLocaleString();

  return (
    <div className="side-panel" style={{ width }}>
      <div
        className="side-panel-resize-handle"
        onMouseDown={handleResizeStart}
      />
      <div className="side-panel-header">
        <div className="side-panel-title">
          <span className={`side-panel-badge ${isAgent ? 'agent' : 'user'}`}>
            {isAgent ? 'Assistant' : 'User'}
          </span>
          <span className="side-panel-timestamp">{formattedTime}</span>
        </div>
        <div className="side-panel-actions">
          {isUserNode && !isEditing && (
            <button
              className="side-panel-edit-button"
              onClick={handleEdit}
              title="Edit content"
            >
              Edit
            </button>
          )}
          {isEditing && (
            <button
              className="side-panel-done-button"
              onClick={handleDone}
            >
              Done
            </button>
          )}
          <button
            className="side-panel-close"
            onClick={() => setPreviewNode(null)}
            title="Close (Escape)"
          >
            ×
          </button>
        </div>
      </div>
      <div className="side-panel-content">
        {isEditing ? (
          <textarea
            ref={textareaRef}
            className="side-panel-textarea"
            value={editContent}
            onChange={handleContentChange}
            placeholder="Enter your message..."
          />
        ) : data.content ? (
          <MarkdownContent content={data.content} />
        ) : (
          <span className="side-panel-empty">No content</span>
        )}
      </div>
    </div>
  );
}
