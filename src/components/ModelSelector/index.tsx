import type { AgentProvider, ModelInfo } from '../../types';
import './styles.css';

interface ModelSelectorProps {
  provider: AgentProvider;
  value: string | undefined;
  onChange: (modelId: string) => void;
  availableModels: ModelInfo[];
  disabled?: boolean;
  compact?: boolean;
  loading?: boolean;
}

export function ModelSelector({
  value,
  onChange,
  availableModels,
  disabled,
  compact,
  loading,
}: ModelSelectorProps) {
  // Don't show if no models available
  if (!loading && availableModels.length === 0) {
    return null;
  }

  return (
    <select
      className={`model-selector ${compact ? 'compact' : ''}`}
      value={value ?? ''}
      onChange={(e) => onChange(e.target.value)}
      disabled={disabled || loading}
      title="Select model"
    >
      {loading ? (
        <option value="">Loading models...</option>
      ) : (
        <>
          <option value="">Default</option>
          {availableModels.map((model) => (
            <option key={model.model_id} value={model.model_id}>
              {model.display_name}
            </option>
          ))}
        </>
      )}
    </select>
  );
}
