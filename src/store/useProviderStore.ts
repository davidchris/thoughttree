import { create } from 'zustand';
import {
  AgentProvider,
  DEFAULT_PROVIDER,
  ModelInfo,
  ModelPreferences,
  ProviderStatus,
} from '../types';

/**
 * Provider and model configuration. Global preferences live here; per-project
 * model preferences are persisted in the project file and therefore live in
 * useGraphStore alongside the rest of the project state.
 */
interface ProviderState {
  defaultProvider: AgentProvider;
  availableProviders: ProviderStatus[];
  globalModelPreferences: ModelPreferences;
  availableModels: Record<AgentProvider, ModelInfo[]>;

  setDefaultProvider: (provider: AgentProvider) => void;
  setAvailableProviders: (providers: ProviderStatus[]) => void;
  setGlobalModelPreferences: (preferences: ModelPreferences) => void;
  setGlobalModelPreference: (provider: AgentProvider, modelId: string | null) => void;
  setAvailableModels: (provider: AgentProvider, models: ModelInfo[]) => void;
}

export const useProviderStore = create<ProviderState>()((set) => ({
  defaultProvider: DEFAULT_PROVIDER,
  availableProviders: [],
  globalModelPreferences: {},
  availableModels: {} as Record<AgentProvider, ModelInfo[]>,

  setDefaultProvider: (provider) => set({ defaultProvider: provider }),
  setAvailableProviders: (providers) => set({ availableProviders: providers }),
  setGlobalModelPreferences: (preferences) => set({ globalModelPreferences: preferences }),

  setGlobalModelPreference: (provider, modelId) => {
    set((state) => ({
      globalModelPreferences: {
        ...state.globalModelPreferences,
        [provider]: modelId ?? undefined,
      },
    }));
  },

  setAvailableModels: (provider, models) => {
    set((state) => ({
      availableModels: { ...state.availableModels, [provider]: models },
    }));
  },
}));
