import { useEffect } from 'react';
import { useGraphStore } from '../../store/useGraphStore';
import { MarkdownContent } from '../Graph/MarkdownContent';
import './styles.css';

export function SidePanel() {
  const previewNodeId = useGraphStore((state) => state.previewNodeId);
  const nodeData = useGraphStore((state) => state.nodeData);
  const setPreviewNode = useGraphStore((state) => state.setPreviewNode);

  const data = previewNodeId ? nodeData.get(previewNodeId) : null;

  // Handle Escape key
  useEffect(() => {
    if (!previewNodeId) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setPreviewNode(null);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [previewNodeId, setPreviewNode]);

  if (!previewNodeId || !data) {
    return null;
  }

  const isAgent = data.role === 'assistant';
  const formattedTime = new Date(data.timestamp).toLocaleString();

  return (
    <div className="side-panel">
      <div className="side-panel-header">
        <div className="side-panel-title">
          <span className={`side-panel-badge ${isAgent ? 'agent' : 'user'}`}>
            {isAgent ? 'Assistant' : 'User'}
          </span>
          <span className="side-panel-timestamp">{formattedTime}</span>
        </div>
        <button
          className="side-panel-close"
          onClick={() => setPreviewNode(null)}
          title="Close (Escape)"
        >
          ×
        </button>
      </div>
      <div className="side-panel-content">
        {data.content ? (
          <MarkdownContent content={data.content} />
        ) : (
          <span className="side-panel-empty">No content</span>
        )}
      </div>
    </div>
  );
}
