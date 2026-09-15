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
  AttachmentLimits,
  BackendTransport,
  FilePreview,
  FilePreviewResponse,
  FileStat,
  ProjectDoc,
  ProjectEntry,
  RecoveryEntry,
  PromptMessage,
  PromptRequest,
  StreamChunk,
  SummaryRequest,
  SummaryResult,
  Unsubscribe,
  VaultFileStatus,
} from './types';
import { conversationToGraph, parseKagiExport } from '@thoughttree/graph-model';
import type { FileRef } from '@thoughttree/graph-model';
import { KagiImportError, StaleRevisionError } from './types';
import type { KagiImportErrorKind } from './types';

interface BackendMessageImage {
  data: string;
  mime_type: string;
}

interface BackendMessageFile {
  path: string;
  name: string;
  mime_type: string;
  size: number;
}

interface BackendMessage {
  role: string;
  content: string;
  images: BackendMessageImage[] | null;
  files: BackendMessageFile[] | null;
}

interface ChunkPayload {
  node_id: string;
  turn_id: string;
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

function hasAttachments(message: PromptMessage): boolean {
  return Boolean(message.images?.length || message.files?.length);
}

function toBackendMessages(messages: PromptMessage[]): BackendMessage[] {
  return messages
    .filter((message) => message.content.trim().length > 0 || hasAttachments(message))
    .map((message) => ({
      role: message.role,
      content: message.content,
      images: message.images?.map((image) => toBackendImage(image)) || null,
      files: message.files?.map((file) => toBackendFile(file)) || null,
    }));
}

function toBackendImage(image: ImageAttachment): BackendMessageImage {
  return {
    data: image.data,
    mime_type: image.mimeType,
  };
}

function toBackendFile(file: FileRef): BackendMessageFile {
  return {
    path: file.path,
    name: file.name,
    mime_type: file.mimeType,
    size: file.size,
  };
}

interface FileStatPayload {
  size: number;
  modified_epoch_ms: number;
}

type VaultFileStatusPayload =
  | { status: 'ok'; stat: FileStatPayload; mime_type: string; name: string }
  | { status: 'missing' }
  | { status: 'invalid' };

type FilePreviewPayload =
  | { kind: 'image'; data: string; mime_type: string; width: number; height: number }
  | { kind: 'text'; excerpt: string; truncated: boolean }
  | { kind: 'none' };

interface FilePreviewResponsePayload {
  info: { name: string; mime_type: string; size: number; modified_epoch_ms: number };
  preview: FilePreviewPayload;
}

interface AttachmentLimitsPayload {
  image_max_bytes: number;
  image_max_side: number;
  max_images_per_prompt: number;
  preview_text_bytes: number;
}

function toFileStat(payload: FileStatPayload): FileStat {
  return { size: payload.size, modifiedEpochMs: payload.modified_epoch_ms };
}

function toVaultFileStatus(payload: VaultFileStatusPayload): VaultFileStatus {
  if (payload.status !== 'ok') return { status: payload.status };
  return {
    status: 'ok',
    stat: toFileStat(payload.stat),
    mimeType: payload.mime_type,
    name: payload.name,
  };
}

function toFilePreview(payload: FilePreviewPayload): FilePreview {
  switch (payload.kind) {
    case 'image':
      return {
        kind: 'image',
        data: payload.data,
        mimeType: payload.mime_type,
        width: payload.width,
        height: payload.height,
      };
    case 'text':
      return { kind: 'text', excerpt: payload.excerpt, truncated: payload.truncated };
    default:
      return { kind: 'none' };
  }
}

function toFilePreviewResponse(payload: FilePreviewResponsePayload): FilePreviewResponse {
  return {
    info: {
      name: payload.info.name,
      mimeType: payload.info.mime_type,
      size: payload.info.size,
      modifiedEpochMs: payload.info.modified_epoch_ms,
    },
    preview: toFilePreview(payload.preview),
  };
}

function toAttachmentLimits(payload: AttachmentLimitsPayload): AttachmentLimits {
  return {
    imageMaxBytes: payload.image_max_bytes,
    imageMaxSide: payload.image_max_side,
    maxImagesPerPrompt: payload.max_images_per_prompt,
    previewTextBytes: payload.preview_text_bytes,
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

interface TaggedCommandErrorPayload {
  kind: string;
  message?: string;
  [key: string]: unknown;
}

function taggedCommandErrorFromUnknown(error: unknown): TaggedCommandErrorPayload | null {
  if (isRecord(error) && typeof error.kind === 'string') {
    return error as TaggedCommandErrorPayload;
  }
  if (isRecord(error) && isRecord(error.error) && typeof error.error.kind === 'string') {
    return error.error as TaggedCommandErrorPayload;
  }
  return null;
}

function projectCommandErrorFromUnknown(error: unknown): ProjectCommandErrorPayload | null {
  return taggedCommandErrorFromUnknown(error) as ProjectCommandErrorPayload | null;
}

const KAGI_IMPORT_ERROR_KINDS: ReadonlySet<string> = new Set<KagiImportErrorKind>([
  'io',
  'input_too_large',
  'invalid_utf8',
]);

function kagiImportErrorFromUnknown(error: unknown): KagiImportError | null {
  const payload = taggedCommandErrorFromUnknown(error);
  if (!payload || !KAGI_IMPORT_ERROR_KINDS.has(payload.kind)) return null;
  return new KagiImportError(
    payload.kind as KagiImportErrorKind,
    typeof payload.message === 'string' ? payload.message : 'Unable to import Kagi export'
  );
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
          turnId: payload.turn_id,
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

  async saveProjectCopy(path: string, data: string): Promise<[string, string]> {
    return invoke('save_project_copy', { path, data });
  }

  async snapshotProject(path: string | null, data: string): Promise<string> {
    return invoke('snapshot_project', { path, data });
  }

  async listProjectRecovery(): Promise<RecoveryEntry[]> {
    return invoke('list_project_recovery');
  }

  async readProjectRecovery(id: string): Promise<string> {
    return invoke('read_project_recovery', { id });
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
    let text: string;
    try {
      text = await invoke<string>('import_kagi_export', { path });
    } catch (error) {
      throw kagiImportErrorFromUnknown(error) ?? error;
    }
    const conversation = parseKagiExport(text);
    return { title: conversation.importKey, graph: conversationToGraph(conversation) };
  }

  async sendPrompt(req: PromptRequest): Promise<string> {
    const messages = toBackendMessages(req.messages);
    if (messages.length === 0) {
      throw new Error('No valid messages to send');
    }

    await this.ensureListeners();
    return invoke<string>('send_prompt', {
      request: {
        ...req,
        messages,
        provider: req.provider || null,
        modelId: req.modelId || null,
        effort: req.effort || null,
      },
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

  async pickVaultFile(): Promise<string | null> {
    return (await invoke<string | null>('pick_vault_file')) ?? null;
  }

  resolveDroppedFile(absolutePath: string): Promise<string> {
    return invoke<string>('resolve_dropped_file', { absolutePath });
  }

  async statVaultFile(path: string): Promise<VaultFileStatus> {
    return toVaultFileStatus(await invoke<VaultFileStatusPayload>('stat_vault_file', { path }));
  }

  async readVaultFilePreview(path: string): Promise<FilePreviewResponse> {
    return toFilePreviewResponse(
      await invoke<FilePreviewResponsePayload>('read_vault_file_preview', { path })
    );
  }

  async getAttachmentLimits(): Promise<AttachmentLimits> {
    return toAttachmentLimits(await invoke<AttachmentLimitsPayload>('get_attachment_limits'));
  }
}
