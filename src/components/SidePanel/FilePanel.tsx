import { useGraphStore } from '../../store/useGraphStore';
import { useFilePreview } from '../../hooks/useFilePreview';
import { FILE_STATE_LABEL, formatBytes, resolveFileCardState } from '../../lib/fileNodes';
import type { FileNodeData } from '../../types';

/** Side panel body for a file node: where it lives, what state it is in, and the same bounded preview as the card. */
export function FilePanel({ node }: { node: FileNodeData }) {
  const status = useGraphStore((state) => state.fileNodeStatus.get(node.id));
  const preview = useFilePreview(node);
  const cardState = resolveFileCardState(status, preview);
  const stateLabel = FILE_STATE_LABEL[cardState];

  return (
    <div className="side-panel-file">
      <dl className="side-panel-file-meta">
        <div>
          <dt>Path</dt>
          <dd>
            <code>{node.path}</code>
          </dd>
        </div>
        <div>
          <dt>Size</dt>
          <dd>{formatBytes(node.size)}</dd>
        </div>
        <div>
          <dt>Type</dt>
          <dd>{node.mimeType}</dd>
        </div>
        {stateLabel && (
          <div>
            <dt>State</dt>
            <dd className={cardState === 'changed' ? 'side-panel-file-state' : 'side-panel-file-state danger'}>
              {stateLabel}
            </dd>
          </div>
        )}
      </dl>
      <FilePreviewBody preview={preview} name={node.name} />
    </div>
  );
}

function FilePreviewBody({ preview, name }: { preview: ReturnType<typeof useFilePreview>; name: string }) {
  switch (preview.kind) {
    case 'loading':
      return <span className="side-panel-empty">Loading preview…</span>;
    case 'failed':
      return <span className="side-panel-empty">Preview unavailable.</span>;
    case 'refused':
      return <span className="side-panel-empty">{FILE_STATE_LABEL[preview.reason]}</span>;
    case 'ready': {
      const body = preview.response.preview;
      if (body.kind === 'image') {
        return (
          <img
            className="side-panel-file-image"
            src={`data:${body.mimeType};base64,${body.data}`}
            alt={name}
            width={body.width}
            height={body.height}
          />
        );
      }
      if (body.kind === 'text') {
        return (
          <pre className="side-panel-plain-text">
            {body.excerpt}
            {body.truncated ? '\n…' : ''}
          </pre>
        );
      }
      return <span className="side-panel-empty">No preview for this file type. The agent reads it from disk.</span>;
    }
  }
}
