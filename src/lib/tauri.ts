import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { useGraphStore } from '../store/useGraphStore';
import type { AgentProvider, ModelInfo, ModelPreferences, PermissionRequest, ProviderPaths, ProviderStatus } from '../types';

interface ChunkPayload {
  node_id: string;
  chunk: string;
}

interface PermissionPayload {
  id: string;
  tool_type: string;
  tool_name: string;
  description: string;
  options: Array<{ id: string; label: string }>;
}

// Global listener for permission requests
let permissionUnlisten: UnlistenFn | null = null;

export async function initializeListeners(): Promise<void> {
  // Set up permission request listener
  if (!permissionUnlisten) {
    permissionUnlisten = await listen<PermissionPayload>('permission-request', (event) => {
      const payload = event.payload;
      const permission: PermissionRequest = {
        id: payload.id,
        toolType: payload.tool_type,
        toolName: payload.tool_name,
        description: payload.description,
        options: payload.options,
      };
      useGraphStore.getState().setPendingPermission(permission);
    });
  }
}

export async function sendPrompt(
  nodeId: string,
  messages: { role: string; content: string }[],
  onChunk: (chunk: string) => void,
  provider?: AgentProvider,
  modelId?: string
): Promise<string> {
  // Set up listener for streaming chunks
  const unlisten = await listen<ChunkPayload>('stream-chunk', (event) => {
    if (event.payload.node_id === nodeId) {
      onChunk(event.payload.chunk);
    }
  });

  try {
    // Convert messages to tuple format expected by backend
    // Filter out empty messages (e.g., placeholder assistant messages before streaming)
    const messageTuples: [string, string][] = messages
      .filter((m) => m.content.trim().length > 0)
      .map((m) => [m.role, m.content]);

    // Validate we have messages to send
    if (messageTuples.length === 0) {
      throw new Error('No valid messages to send');
    }

    const result = await invoke<string>('send_prompt', {
      nodeId,
      messages: messageTuples,
      provider: provider || null,
      modelId: modelId || null,
    });

    return result;
  } finally {
    unlisten();
  }
}

export async function respondToPermission(requestId: string, optionId: string): Promise<void> {
  await invoke('respond_to_permission', {
    requestId,
    optionId,
  });
}

export async function checkAcpAvailable(): Promise<boolean> {
  return invoke<boolean>('check_acp_available');
}

export async function searchFiles(query: string, limit?: number): Promise<string[]> {
  return invoke<string[]>('search_files', { query, limit });
}

// ============================================================================
// Provider management
// ============================================================================

export async function getAvailableProviders(): Promise<ProviderStatus[]> {
  return invoke<ProviderStatus[]>('get_available_providers');
}

export async function getDefaultProvider(): Promise<AgentProvider> {
  return invoke<AgentProvider>('get_default_provider');
}

export async function setDefaultProvider(provider: AgentProvider): Promise<void> {
  await invoke('set_default_provider', { provider });
}

// ============================================================================
// Model management
// ============================================================================

export async function getModelPreferences(): Promise<ModelPreferences> {
  return invoke<ModelPreferences>('get_model_preferences');
}

export async function setModelPreference(
  provider: AgentProvider,
  modelId: string | null
): Promise<void> {
  await invoke('set_model_preference', { provider, modelId });
}

export async function getAvailableModels(provider: AgentProvider): Promise<ModelInfo[]> {
  return invoke<ModelInfo[]>('get_available_models', { provider });
}

// ============================================================================
// Provider path configuration
// ============================================================================

export async function getProviderPaths(): Promise<ProviderPaths> {
  return invoke<ProviderPaths>('get_provider_paths');
}

export async function setProviderPath(
  provider: AgentProvider,
  path: string | null
): Promise<void> {
  await invoke('set_provider_path', { provider, path });
}

export async function validateProviderPath(
  provider: AgentProvider,
  path: string
): Promise<string> {
  return invoke<string>('validate_provider_path', { provider, path });
}

export async function pickProviderExecutable(
  provider: AgentProvider
): Promise<string | null> {
  return invoke<string | null>('pick_provider_executable', { provider });
}
