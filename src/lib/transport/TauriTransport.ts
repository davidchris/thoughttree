import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { withoutNullEntries } from '../../types';
import type {
  AgentProvider,
  EffortPreferences,
  ImageAttachment,
  ModelInfo,
  ModelPreferences,
  PermissionRequest,
  ProviderStatus,
  ReasoningEffort,
  StoredProviderRecord,
} from '../../types';
import type {
  BackendTransport,
  ProjectDoc,
  ProjectEntry,
  PromptMessage,
  PromptRequest,
  StreamChunk,
  SummaryRequest,
  SummaryResult,
  Unsubscribe,
} from './types';
import { conversationToGraph, parseKagiExport } from '@thoughttree/graph-model';
import { StaleRevisionError } from './types';

interface BackendMessageImage {
  data: string;
  mime_type: string;
}

interface BackendMessage {
  role: string;
  content: string;
  images: BackendMessageImage[] | null;
}

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

interface LoadProjectPayload {
  content: string;
  revision: string;
}

interface ProjectCommandErrorPayload {
  kind: 'message' | 'stale_revision';
  message?: string;
  current_revision?: string;
}

function toBackendMessages(messages: PromptMessage[]): BackendMessage[] {
  return messages
    .filter((message) => message.content.trim().length > 0 || (message.images && message.images.length > 0))
    .map((message) => ({
      role: message.role,
      content: message.content,
      images: message.images?.map((image) => toBackendImage(image)) || null,
    }));
}

function toBackendImage(image: ImageAttachment): BackendMessageImage {
  return {
    data: image.data,
    mime_type: image.mimeType,
  };
}

function toPermissionRequest(payload: PermissionPayload): PermissionRequest {
  return {
    id: payload.id,
    toolType: payload.tool_type,
    toolName: payload.tool_name,
    description: payload.description,
    options: payload.options,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function projectCommandErrorFromUnknown(error: unknown): ProjectCommandErrorPayload | null {
  if (isRecord(error) && typeof error.kind === 'string') {
    return error as unknown as ProjectCommandErrorPayload;
  }
  if (isRecord(error) && isRecord(error.error) && typeof error.error.kind === 'string') {
    return error.error as unknown as ProjectCommandErrorPayload;
  }
  return null;
}

export class TauriTransport implements BackendTransport {
  readonly capabilities = { nativeDialogs: true } as const;

  private readonly streamChunkSubscribers = new Set<(ev: StreamChunk) => void>();
  private readonly permissionSubscribers = new Set<(ev: PermissionRequest) => void>();
  private listenersReady: Promise<void>;

  constructor() {
    this.listenersReady = this.initializeListeners();
  }

  private async initializeListeners(): Promise<void> {
    await Promise.all([
      listen<ChunkPayload>('stream-chunk', (event) => {
        const payload = event.payload;
        const chunk: StreamChunk = {
          nodeId: payload.node_id,
          chunk: payload.chunk,
        };
        for (const subscriber of this.streamChunkSubscribers) {
          subscriber(chunk);
        }
      }),
      listen<PermissionPayload>('permission-request', (event) => {
        const permission = toPermissionRequest(event.payload);
        for (const subscriber of this.permissionSubscribers) {
          subscriber(permission);
        }
      }),
    ]);
  }

  private async ensureListeners(): Promise<void> {
    await this.listenersReady;
  }

  onStreamChunk(cb: (ev: StreamChunk) => void): Unsubscribe {
    this.streamChunkSubscribers.add(cb);
    void this.ensureListeners();
    return () => {
      this.streamChunkSubscribers.delete(cb);
    };
  }

  onPermissionRequest(cb: (ev: PermissionRequest) => void): Unsubscribe {
    this.permissionSubscribers.add(cb);
    void this.ensureListeners();
    return () => {
      this.permissionSubscribers.delete(cb);
    };
  }

  async loadProject(path: string): Promise<ProjectDoc> {
    const project = await invoke<LoadProjectPayload>('load_project', { path });
    return { data: project.content, revision: project.revision };
  }

  async saveProject(path: string, data: string, baseRevision: string | null): Promise<string> {
    try {
      return await invoke<string>('save_project', { path, data, baseRevision });
    } catch (error) {
      const payload = projectCommandErrorFromUnknown(error);
      if (payload?.kind === 'stale_revision' && payload.current_revision) {
        throw new StaleRevisionError(payload.current_revision);
      }
      throw error;
    }
  }

  async listProjects(): Promise<ProjectEntry[]> {
    return invoke<ProjectEntry[]>('list_projects');
  }

  async importKagiExport(path: string): Promise<import('./types').ImportedGraph> {
    const text = await invoke<string>('import_kagi_export', { path });
    const conversation = parseKagiExport(text);
    return { title: conversation.importKey, graph: conversationToGraph(conversation) };
  }

  async sendPrompt(req: PromptRequest): Promise<string> {
    const messages = toBackendMessages(req.messages);
    if (messages.length === 0) {
      throw new Error('No valid messages to send');
    }

    return invoke<string>('send_prompt', {
      nodeId: req.nodeId,
      messages,
      provider: req.provider || null,
      modelId: req.modelId || null,
      effort: req.effort || null,
    });
  }

  async respondToPermission(requestId: string, optionId: string): Promise<void> {
    await invoke('respond_to_permission', { requestId, optionId });
  }

  checkAcpAvailable(): Promise<boolean> {
    return invoke<boolean>('check_acp_available');
  }

  searchFiles(query: string, limit?: number): Promise<string[]> {
    return invoke<string[]>('search_files', { query, limit });
  }

  getAvailableProviders(): Promise<ProviderStatus[]> {
    return invoke<ProviderStatus[]>('get_available_providers');
  }

  getDefaultProvider(): Promise<AgentProvider> {
    return invoke<AgentProvider>('get_default_provider');
  }

  async setDefaultProvider(provider: AgentProvider): Promise<void> {
    await invoke('set_default_provider', { provider });
  }

  async getModelPreferences(): Promise<ModelPreferences> {
    return withoutNullEntries(await invoke<StoredProviderRecord>('get_model_preferences'));
  }

  async setModelPreference(provider: AgentProvider, modelId: string | null): Promise<void> {
    await invoke('set_model_preference', { provider, modelId });
  }

  async getEffortPreferences(): Promise<EffortPreferences> {
    return withoutNullEntries(
      await invoke<StoredProviderRecord<ReasoningEffort>>('get_effort_preferences')
    );
  }

  async setEffortPreference(
    provider: AgentProvider,
    effort: ReasoningEffort | null
  ): Promise<void> {
    await invoke('set_effort_preference', { provider, effort });
  }

  getAvailableModels(provider: AgentProvider): Promise<ModelInfo[]> {
    return invoke<ModelInfo[]>('get_available_models', { provider });
  }

  generateSummary(req: SummaryRequest): Promise<SummaryResult> {
    return invoke<SummaryResult>('generate_summary', {
      nodeId: req.nodeId,
      content: req.content,
    });
  }
}
