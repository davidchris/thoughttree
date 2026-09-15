import { useEffect } from 'react';
import { Handle, Position, NodeProps } from '@xyflow/react';
import { useGraphStore } from '../../store/useGraphStore';
import { useUIStore } from '../../store/useUIStore';
import { useFilePreview, type PreviewState } from '../../hooks/useFilePreview';
import {
  FILE_STATE_LABEL,
  fileTypeBadge,
  formatBytes,
  isRasterImage,
  resolveFileCardState,
  type FileCardState,
} from '../../lib/fileNodes';
import './styles.css';

export function FileNode({ id, selected }: NodeProps) {
  const node = useGraphStore((state) => {
    const data = state.nodeData.get(id);
    return data?.role === 'file' ? data : undefined;
  });
  const status = useGraphStore((state) => state.fileNodeStatus.get(id));
  const refreshFileNodeStat = useGraphStore((state) => state.refreshFileNodeStat);
  const acknowledgeFileChange = useGraphStore((state) => state.acknowledgeFileChange);
  const isNodeBlocked = useGraphStore((state) => state.isNodeBlocked);
  const setPreviewNode = useUIStore((state) => state.setPreviewNode);
  const isFlashing = useUIStore((state) => state.flashNodeId === id);
  const preview = useFilePreview(node);

  // A freshly linked or freshly loaded node has no status yet: stat it once.
  const hasStatus = status !== undefined;
  useEffect(() => {
    if (!hasStatus) void refreshFileNodeStat(id);
  }, [id, hasStatus, refreshFileNodeStat]);

  if (!node) return null;

  const cardState = resolveFileCardState(status, preview);
  const isImage = isRasterImage(node.mimeType);
  const isBlocked = isNodeBlocked(id);
  const badge = fileTypeBadge(node.name);

  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setPreviewNode(id);
  };

  const handleRefresh = (e: React.MouseEvent) => {
    e.stopPropagation();
    void acknowledgeFileChange(id);
  };

  return (
    <div
      className={`thought-node file-node state-${cardState} ${selected ? 'selected' : ''} ${isBlocked ? 'blocked' : ''} ${isFlashing ? 'flash' : ''}`}
      onDoubleClick={handleDoubleClick}
    >
      <div className="node-header">
        <span className="node-role file-type-badge">{badge}</span>
        <span className="file-node-name" title={node.path}>
          {node.name}
        </span>
        <span className="file-node-size">{formatBytes(node.size)}</span>
      </div>

      <div className="file-node-preview">
        <PreviewBody preview={preview} name={node.name} badge={badge} cardState={cardState} />
      </div>

      <div className="file-node-footer">
        {cardState === 'changed' ? (
          <>
            <span className="file-node-flag" title="Reload to update the preview and what the agent will see">
              {FILE_STATE_LABEL.changed}
            </span>
            <button className="file-node-refresh" onClick={handleRefresh} title="Adopt the current file version">
              Reload
            </button>
          </>
        ) : FILE_STATE_LABEL[cardState] ? (
          <span className="file-node-flag danger">{FILE_STATE_LABEL[cardState]}</span>
        ) : (
          !isImage && <span className="file-node-hint">agent reads from disk</span>
        )}
      </div>

      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}

interface PreviewBodyProps {
  preview: PreviewState;
  name: string;
  badge: string;
  cardState: FileCardState;
}

function PreviewBody({ preview, name, badge, cardState }: PreviewBodyProps) {
  if (preview.kind === 'ready') {
    const { preview: body } = preview.response;
    if (body.kind === 'image') {
      return <img src={`data:${body.mimeType};base64,${body.data}`} alt={name} draggable={false} />;
    }
    if (body.kind === 'text') {
      return (
        <pre className="file-node-excerpt">
          {body.excerpt}
          {body.truncated ? '…' : ''}
        </pre>
      );
    }
  }
  // Loading, no preview for this type, or a broken file: a ghosted badge keeps
  // the card recognisable at a glance.
  return <span className={`file-node-ghost ${cardState === 'ok' || cardState === 'changed' ? '' : 'broken'}`}>{badge}</span>;
}
