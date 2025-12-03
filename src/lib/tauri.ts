import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { useGraphStore } from '../store/useGraphStore';
import { PermissionRequest } from '../types';

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
  onChunk: (chunk: string) => void
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
