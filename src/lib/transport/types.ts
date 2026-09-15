import type {
  AgentProvider,
  EffortPreferences,
  ImageAttachment,
  ModelInfo,
  ModelPreferences,
  PermissionRequest,
  ProviderStatus,
  ReasoningEffort,
} from '../../types';
import type { FileRef, Graph } from '../../../packages/graph-model/src/types';

export interface ProjectDoc {
  data: string;
  revision: string;
}

export interface RecoveryEntry {
  id: string;
  sourcePath: string | null;
  createdEpochMs: number;
}

export interface ProjectEntry {
  relativePath: string;
  modifiedEpochMs: number;
}

export interface ImportedGraph {
  title: string;
  graph: Graph;
}

export class StaleRevisionError extends Error {
  constructor(public currentRevision: string) {
    super('project file changed since last read');
    this.name = 'StaleRevisionError';
  }
}

export type KagiImportErrorKind = 'io' | 'input_too_large' | 'invalid_utf8';

/** Typed failure from the backend Kagi import file seam (before parsing). */
export class KagiImportError extends Error {
  constructor(
    public readonly kind: KagiImportErrorKind,
    message: string
  ) {
    super(message);
    this.name = 'KagiImportError';
  }
}

export type Unsubscribe = () => void;

export interface PromptMessage {
  role: string;
  content: string;
  images?: ImageAttachment[];
  /** Vault file references contributed by file nodes; the backend reads them at send time. */
  files?: FileRef[];
}

export interface PromptRequest {
  nodeId: string;
  messages: PromptMessage[];
  provider?: AgentProvider;
  modelId?: string;
  effort?: ReasoningEffort;
}

export interface StreamChunk {
  nodeId: string;
  chunk: string;
}

/** Turn provenance captured by the backend; `provenance` is untrusted until normalized by the graph model. */
export interface TurnProvenanceEvent {
  nodeId: string;
  provenance: unknown;
}

export interface SummaryRequest {
  nodeId: string;
  content: string;
}

export interface SummaryResult {
  node_id: string;
  summary: string;
}

/** Size and mtime of a Vault file as seen by the backend (File node staleness check). */
export interface FileStat {
  size: number;
  modifiedEpochMs: number;
}

/** Stat outcome for a File node; missing/invalid are states, not errors, so the UI can render them. */
export type VaultFileStatus =
  | { status: 'ok'; stat: FileStat; mimeType: string; name: string }
  | { status: 'missing' }
  | { status: 'invalid' };

export interface FileInfo {
  name: string;
  mimeType: string;
  size: number;
  modifiedEpochMs: number;
}

export type FilePreview =
  | { kind: 'image'; data: string; mimeType: string; width: number; height: number }
  | { kind: 'text'; excerpt: string; truncated: boolean }
  | { kind: 'none' };

export interface FilePreviewResponse {
  info: FileInfo;
  preview: FilePreview;
}

/** Attachment limits owned by the core crate; never duplicate the numbers in the frontend. */
export interface AttachmentLimits {
  imageMaxBytes: number;
  imageMaxSide: number;
  maxImagesPerPrompt: number;
  previewTextBytes: number;
}

export interface BackendTransport {
  loadProject(path: string): Promise<ProjectDoc>;
  saveProject(path: string, data: string, baseRevision: string | null): Promise<string>;
  saveProjectCopy(path: string, data: string): Promise<[string, string]>;
  snapshotProject(path: string | null, data: string): Promise<string>;
  listProjectRecovery(): Promise<RecoveryEntry[]>;
  readProjectRecovery(id: string): Promise<string>;
  listProjects(): Promise<ProjectEntry[]>;
  importKagiExport(path: string): Promise<ImportedGraph>;

  sendPrompt(req: PromptRequest): Promise<string>;
  respondToPermission(requestId: string, optionId: string): Promise<void>;
  checkAcpAvailable(): Promise<boolean>;
  searchFiles(query: string, limit?: number): Promise<string[]>;
  getAvailableProviders(): Promise<ProviderStatus[]>;
  getDefaultProvider(): Promise<AgentProvider>;
  setDefaultProvider(provider: AgentProvider): Promise<void>;
  getModelPreferences(): Promise<ModelPreferences>;
  setModelPreference(provider: AgentProvider, modelId: string | null): Promise<void>;
  getEffortPreferences(): Promise<EffortPreferences>;
  setEffortPreference(provider: AgentProvider, effort: ReasoningEffort | null): Promise<void>;
  getAvailableModels(provider: AgentProvider): Promise<ModelInfo[]>;
  generateSummary(req: SummaryRequest): Promise<SummaryResult>;

  /** Native picker rooted at the Vault; resolves to a Vault-relative path, or null when cancelled. */
  pickVaultFile(): Promise<string | null>;
  /** Vault-relative path for a dropped absolute path; rejects when the file is outside the Vault. */
  resolveDroppedFile(absolutePath: string): Promise<string>;
  statVaultFile(path: string): Promise<VaultFileStatus>;
  /** Rejects with a string starting `too_large:`, `missing:` or `invalid:`. */
  readVaultFilePreview(path: string): Promise<FilePreviewResponse>;
  getAttachmentLimits(): Promise<AttachmentLimits>;

  onStreamChunk(cb: (ev: StreamChunk) => void): Unsubscribe;
  onPermissionRequest(cb: (ev: PermissionRequest) => void): Unsubscribe;
  /** Fires once per Turn, after the prompt settles and before `sendPrompt` resolves. */
  onTurnProvenance(cb: (ev: TurnProvenanceEvent) => void): Unsubscribe;

  readonly capabilities: { nativeDialogs: boolean };
}
