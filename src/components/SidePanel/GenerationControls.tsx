import { useCallback, useEffect, useState } from 'react';
import { useProviderStore } from '../../store/useProviderStore';
import { ProviderSelector } from '../ProviderSelector';
import { ModelSelector } from '../ModelSelector';
import { getAvailableModels } from '../../lib/tauri';
import type { AgentProvider } from '../../types';
import { logger } from '../../lib/logger';

interface GenerationControlsProps {
  provider: AgentProvider;
  model: string | undefined;
  onProviderChange: (provider: AgentProvider) => void;
  onModelChange: (model: string | undefined) => void;
  disabled: boolean;
  generateDisabled: boolean;
  onGenerate: () => void;
}

/**
 * Provider/model pickers plus the Generate button. Lazily fetches the model
 * catalog for the selected provider.
 */
export function GenerationControls({
  provider,
  model,
  onProviderChange,
  onModelChange,
  disabled,
  generateDisabled,
  onGenerate,
}: GenerationControlsProps) {
  const availableProviders = useProviderStore((state) => state.availableProviders);
  const availableModels = useProviderStore((state) => state.availableModels);
  const setAvailableModels = useProviderStore((state) => state.setAvailableModels);
  const [loadingModels, setLoadingModels] = useState(false);

  const fetchModels = useCallback(
    async (target: AgentProvider) => {
      // Skip if already loaded
      if (availableModels[target]?.length > 0) return;

      setLoadingModels(true);
      try {
        const models = await getAvailableModels(target);
        setAvailableModels(target, models);
      } catch (error) {
        logger.error('Failed to fetch models:', error);
      } finally {
        setLoadingModels(false);
      }
    },
    [availableModels, setAvailableModels]
  );

  useEffect(() => {
    fetchModels(provider);
  }, [provider, fetchModels]);

  return (
    <>
      {availableProviders.length > 0 && (
        <ProviderSelector
          value={provider}
          onChange={(next) => {
            onProviderChange(next);
            // Reset model selection when provider changes
            onModelChange(undefined);
          }}
          availableProviders={availableProviders}
          disabled={disabled}
          compact
        />
      )}
      <ModelSelector
        provider={provider}
        value={model}
        onChange={onModelChange}
        availableModels={availableModels[provider] ?? []}
        disabled={disabled}
        loading={loadingModels}
        compact
      />
      <button
        className="side-panel-generate-button"
        onClick={onGenerate}
        disabled={generateDisabled}
        title="Generate response (Cmd+Enter)"
      >
        {disabled ? 'Generating...' : 'Generate'}
      </button>
    </>
  );
}
