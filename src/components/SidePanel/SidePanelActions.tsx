import type { AgentProvider } from '../../types';
import { CopyButton } from './CopyButton';
import { GenerationControls } from './GenerationControls';

interface SidePanelActionsProps {
  content: string;
  isUserNode: boolean;
  isEditing: boolean;
  isBlocked: boolean;
  /** Text, images, or a file node in the lineage, and no blocker. */
  canGenerate: boolean;
  /** Why generating is refused right now; shown as the Generate tooltip. */
  generateBlockedReason: string | null;
  provider: AgentProvider;
  model: string | undefined;
  onProviderChange: (provider: AgentProvider) => void;
  onModelChange: (model: string | undefined) => void;
  onGenerate: () => void;
  onStartEdit: () => void;
  onFinishEdit: () => void;
  onClose: () => void;
}

/** Toolbar of the side panel header: copy, edit/done, generation controls, close. */
export function SidePanelActions({
  content,
  isUserNode,
  isEditing,
  isBlocked,
  canGenerate,
  generateBlockedReason,
  provider,
  model,
  onProviderChange,
  onModelChange,
  onGenerate,
  onStartEdit,
  onFinishEdit,
  onClose,
}: SidePanelActionsProps) {
  return (
    <div className="side-panel-actions">
      {!isEditing && content && <CopyButton content={content} />}
      {isUserNode && !isEditing && (
        <button
          className="side-panel-edit-button"
          onClick={onStartEdit}
          title="Edit content"
        >
          Edit
        </button>
      )}
      {isUserNode && (
        <GenerationControls
          provider={provider}
          model={model}
          onProviderChange={onProviderChange}
          onModelChange={onModelChange}
          disabled={isBlocked}
          generateDisabled={isBlocked || !canGenerate}
          generateBlockedReason={generateBlockedReason}
          onGenerate={onGenerate}
        />
      )}
      {isEditing && (
        <button className="side-panel-done-button" onClick={onFinishEdit}>
          Done
        </button>
      )}
      <button className="side-panel-close" onClick={onClose} title="Close (Escape)">
        ×
      </button>
    </div>
  );
}
