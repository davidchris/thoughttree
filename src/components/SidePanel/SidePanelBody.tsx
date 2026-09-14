import { MarkdownContent } from '../Graph/MarkdownContent';
import type { AgentNodeData, ImageAttachment } from '../../types';
import { EditArea } from './EditArea';
import { Provenance } from './Provenance';

interface SidePanelBodyProps {
  nodeId: string;
  content: string;
  images: ImageAttachment[];
  isEditing: boolean;
  isStreaming: boolean;
  provenance: AgentNodeData['provenance'];
  onGenerate: () => void;
}

/** Placeholder shown when a node has no content yet. */
function EmptyContent({ isStreaming }: { isStreaming: boolean }) {
  return (
    <span className="side-panel-empty">
      {isStreaming ? 'Waiting for response...' : 'No content'}
    </span>
  );
}

/** Content area of the side panel: editor, or rendered content plus provenance. */
export function SidePanelBody({
  nodeId,
  content,
  images,
  isEditing,
  isStreaming,
  provenance,
  onGenerate,
}: SidePanelBodyProps) {
  return (
    <div className="side-panel-content">
      {isEditing ? (
        <EditArea
          nodeId={nodeId}
          initialContent={content}
          images={images}
          onGenerate={onGenerate}
        />
      ) : (
        <>
          {content ? (
            // Streaming content is shown raw: partial markdown renders badly
            isStreaming ? (
              <pre className="side-panel-plain-text">{content}</pre>
            ) : (
              <MarkdownContent content={content} />
            )
          ) : (
            <EmptyContent isStreaming={isStreaming} />
          )}
          {provenance && (
            <Provenance key={nodeId} provenance={provenance} content={content} />
          )}
        </>
      )}
    </div>
  );
}
