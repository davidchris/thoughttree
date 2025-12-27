import { useEffect, useState, useCallback } from 'react';
import { useGraphStore } from '../../store/useGraphStore';
import {
  getAvailableModels,
  getAvailableProviders,
  getModelPreferences,
  getProviderPaths,
  pickProviderExecutable,
  setModelPreference,
  setProviderPath,
  validateProviderPath,
} from '../../lib/tauri';
import { ModelSelector } from '../ModelSelector';
import { PROVIDER_DISPLAY_NAMES, type AgentProvider, type ProviderPaths } from '../../types';
import './styles.css';

interface SettingsDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

const PROVIDERS: AgentProvider[] = ['claude-code', 'gemini-cli'];

interface PathValidationState {
  status: 'idle' | 'validating' | 'valid' | 'invalid';
  message?: string;
}

export function SettingsDialog({ isOpen, onClose }: SettingsDialogProps) {
  const projectPath = useGraphStore((state) => state.projectPath);
  const availableProviders = useGraphStore((state) => state.availableProviders);
  const setAvailableProviders = useGraphStore((state) => state.setAvailableProviders);
  const globalModelPreferences = useGraphStore((state) => state.globalModelPreferences);
  const projectModelPreferences = useGraphStore((state) => state.projectModelPreferences);
  const setGlobalModelPreferences = useGraphStore((state) => state.setGlobalModelPreferences);
  const setGlobalModelPreference = useGraphStore((state) => state.setGlobalModelPreference);
  const setProjectModelPreference = useGraphStore((state) => state.setProjectModelPreference);
  const availableModels = useGraphStore((state) => state.availableModels);
  const setAvailableModels = useGraphStore((state) => state.setAvailableModels);

  const [loadingModels, setLoadingModels] = useState<Record<AgentProvider, boolean>>({
    'claude-code': false,
    'gemini-cli': false,
  });

  // Provider path state
  const [providerPaths, setProviderPathsState] = useState<ProviderPaths>({});
  const [pathInputs, setPathInputs] = useState<ProviderPaths>({});
  const [pathValidation, setPathValidation] = useState<Record<AgentProvider, PathValidationState>>({
    'claude-code': { status: 'idle' },
    'gemini-cli': { status: 'idle' },
  });

  // Load global preferences and provider paths on mount
  useEffect(() => {
    if (isOpen) {
      getModelPreferences().then(setGlobalModelPreferences).catch(console.error);
      getProviderPaths().then((paths) => {
        setProviderPathsState(paths);
        setPathInputs(paths);
        // Set validation status based on current availability
        PROVIDERS.forEach((provider) => {
          const isAvailable = availableProviders.some(
            (p) => p.provider === provider && p.available
          );
          const errorMsg = availableProviders.find((p) => p.provider === provider)?.error_message;
          setPathValidation((prev) => ({
            ...prev,
            [provider]: {
              status: isAvailable ? 'valid' : (paths[provider] ? 'invalid' : 'idle'),
              message: isAvailable ? 'Found' : errorMsg || undefined,
            },
          }));
        });
      }).catch(console.error);
    }
  }, [isOpen, setGlobalModelPreferences, availableProviders]);

  // Fetch models for a provider
  const fetchModels = useCallback(async (provider: AgentProvider) => {
    if (availableModels[provider]?.length > 0) return;

    setLoadingModels((prev) => ({ ...prev, [provider]: true }));
    try {
      const models = await getAvailableModels(provider);
      setAvailableModels(provider, models);
    } catch (error) {
      console.error(`Failed to fetch models for ${provider}:`, error);
    } finally {
      setLoadingModels((prev) => ({ ...prev, [provider]: false }));
    }
  }, [availableModels, setAvailableModels]);

  // Fetch models for available providers on open
  useEffect(() => {
    if (!isOpen) return;

    PROVIDERS.forEach((provider) => {
      const isAvailable = availableProviders.some(
        (p) => p.provider === provider && p.available
      );
      if (isAvailable) {
        fetchModels(provider);
      }
    });
  }, [isOpen, availableProviders, fetchModels]);

  const handleGlobalModelChange = async (provider: AgentProvider, modelId: string) => {
    const newModelId = modelId || null;
    setGlobalModelPreference(provider, newModelId);
    try {
      await setModelPreference(provider, newModelId);
    } catch (error) {
      console.error('Failed to save model preference:', error);
    }
  };

  const handleProjectModelChange = (provider: AgentProvider, modelId: string) => {
    const newModelId = modelId || null;
    setProjectModelPreference(provider, newModelId);
  };

  // Path management handlers
  const handlePathInputChange = (provider: AgentProvider, value: string) => {
    setPathInputs((prev) => ({ ...prev, [provider]: value || undefined }));
  };

  const handlePathBlur = async (provider: AgentProvider) => {
    const path = pathInputs[provider];

    // If empty and different from saved, reset
    if (!path) {
      if (providerPaths[provider]) {
        await handleResetPath(provider);
      }
      return;
    }

    // If unchanged, don't validate
    if (path === providerPaths[provider]) {
      return;
    }

    // Validate the path
    setPathValidation((prev) => ({ ...prev, [provider]: { status: 'validating' } }));
    try {
      const version = await validateProviderPath(provider, path);
      await setProviderPath(provider, path);
      setProviderPathsState((prev) => ({ ...prev, [provider]: path }));
      setPathValidation((prev) => ({
        ...prev,
        [provider]: { status: 'valid', message: version },
      }));
      // Refresh provider availability
      const providers = await getAvailableProviders();
      setAvailableProviders(providers);
    } catch (error) {
      setPathValidation((prev) => ({
        ...prev,
        [provider]: { status: 'invalid', message: String(error) },
      }));
    }
  };

  const handleBrowse = async (provider: AgentProvider) => {
    try {
      const path = await pickProviderExecutable(provider);
      if (path) {
        setPathInputs((prev) => ({ ...prev, [provider]: path }));
        // Validate and save
        setPathValidation((prev) => ({ ...prev, [provider]: { status: 'validating' } }));
        try {
          const version = await validateProviderPath(provider, path);
          await setProviderPath(provider, path);
          setProviderPathsState((prev) => ({ ...prev, [provider]: path }));
          setPathValidation((prev) => ({
            ...prev,
            [provider]: { status: 'valid', message: version },
          }));
          // Refresh provider availability
          const providers = await getAvailableProviders();
          setAvailableProviders(providers);
        } catch (error) {
          setPathValidation((prev) => ({
            ...prev,
            [provider]: { status: 'invalid', message: String(error) },
          }));
        }
      }
    } catch (error) {
      console.error('Failed to pick executable:', error);
    }
  };

  const handleResetPath = async (provider: AgentProvider) => {
    try {
      await setProviderPath(provider, null);
      setProviderPathsState((prev) => {
        const next = { ...prev };
        delete next[provider];
        return next;
      });
      setPathInputs((prev) => {
        const next = { ...prev };
        delete next[provider];
        return next;
      });
      setPathValidation((prev) => ({ ...prev, [provider]: { status: 'idle' } }));
      // Refresh provider availability
      const providers = await getAvailableProviders();
      setAvailableProviders(providers);
    } catch (error) {
      console.error('Failed to reset path:', error);
    }
  };

  // Close on Escape
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return (
    <div className="settings-dialog-overlay" onClick={onClose}>
      <div className="settings-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="settings-dialog-header">
          <h1>Settings</h1>
          <button className="settings-dialog-close" onClick={onClose} title="Close (Escape)">
            x
          </button>
        </div>

        <div className="settings-section">
          <h2>Provider Executables</h2>
          <p className="settings-description">
            Configure custom paths to CLI executables. Leave empty for auto-detection.
          </p>

          <div className="settings-grid">
            {PROVIDERS.map((provider) => {
              const validation = pathValidation[provider];
              const hasCustomPath = !!pathInputs[provider];

              return (
                <div key={provider} className="settings-row provider-path-row">
                  <label className="settings-label">
                    {PROVIDER_DISPLAY_NAMES[provider]}
                  </label>
                  <div className="provider-path-controls">
                    <input
                      type="text"
                      className="provider-path-input"
                      placeholder="(auto-detect)"
                      value={pathInputs[provider] ?? ''}
                      onChange={(e) => handlePathInputChange(provider, e.target.value)}
                      onBlur={() => handlePathBlur(provider)}
                      disabled={validation.status === 'validating'}
                    />
                    <button
                      className="provider-path-button"
                      onClick={() => handleBrowse(provider)}
                      disabled={validation.status === 'validating'}
                      title="Browse for executable"
                    >
                      Browse
                    </button>
                    {hasCustomPath && (
                      <button
                        className="provider-path-button provider-path-reset"
                        onClick={() => handleResetPath(provider)}
                        disabled={validation.status === 'validating'}
                        title="Reset to auto-detect"
                      >
                        Reset
                      </button>
                    )}
                  </div>
                  <div
                    className={`provider-path-status ${
                      validation.status === 'valid'
                        ? 'status-valid'
                        : validation.status === 'invalid'
                        ? 'status-invalid'
                        : validation.status === 'validating'
                        ? 'status-validating'
                        : ''
                    }`}
                  >
                    {validation.status === 'validating' && 'Validating...'}
                    {validation.status === 'valid' && `✓ ${validation.message}`}
                    {validation.status === 'invalid' && `✗ ${validation.message}`}
                    {validation.status === 'idle' && !hasCustomPath && (
                      availableProviders.some((p) => p.provider === provider && p.available)
                        ? '✓ Auto-detected'
                        : availableProviders.find((p) => p.provider === provider)?.error_message || 'Not found'
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        <div className="settings-section">
          <h2>Default Models (Global)</h2>
          <p className="settings-description">
            Set the default model for each provider. These settings apply to all projects.
          </p>

          <div className="settings-grid">
            {PROVIDERS.map((provider) => {
              const isAvailable = availableProviders.some(
                (p) => p.provider === provider && p.available
              );

              return (
                <div key={provider} className="settings-row">
                  <label className="settings-label">
                    {PROVIDER_DISPLAY_NAMES[provider]}
                  </label>
                  {isAvailable ? (
                    <ModelSelector
                      provider={provider}
                      value={globalModelPreferences[provider]}
                      onChange={(modelId) => handleGlobalModelChange(provider, modelId)}
                      availableModels={availableModels[provider] ?? []}
                      loading={loadingModels[provider]}
                    />
                  ) : (
                    <span className="settings-unavailable">Not installed</span>
                  )}
                </div>
              );
            })}
          </div>
        </div>

        {projectPath && (
          <div className="settings-section">
            <h2>Project Models (Override)</h2>
            <p className="settings-description">
              Override the default model for this project. Leave as &quot;Default&quot; to use global settings.
            </p>

            <div className="settings-grid">
              {PROVIDERS.map((provider) => {
                const isAvailable = availableProviders.some(
                  (p) => p.provider === provider && p.available
                );

                return (
                  <div key={provider} className="settings-row">
                    <label className="settings-label">
                      {PROVIDER_DISPLAY_NAMES[provider]}
                    </label>
                    {isAvailable ? (
                      <ModelSelector
                        provider={provider}
                        value={projectModelPreferences?.[provider]}
                        onChange={(modelId) => handleProjectModelChange(provider, modelId)}
                        availableModels={availableModels[provider] ?? []}
                        loading={loadingModels[provider]}
                      />
                    ) : (
                      <span className="settings-unavailable">Not installed</span>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}

        <div className="settings-dialog-footer">
          <button className="settings-done-button" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
