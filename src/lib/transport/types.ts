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
import type { Graph } from '../../../packages/graph-model/src/types';

export interface ProjectDoc {
  data: string;
  revision: string;
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

export type Unsubscribe = () => void;

export interface PromptMessage {
  role: string;
  content: string;
  images?: ImageAttachment[];
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

export interface SummaryRequest {
  nodeId: string;
  content: string;
}

export interface SummaryResult {
  node_id: string;
  summary: string;
}

export interface BackendTransport {
  loadProject(path: string): Promise<ProjectDoc>;
  saveProject(path: string, data: string, baseRevision: string | null): Promise<string>;
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

  onStreamChunk(cb: (ev: StreamChunk) => void): Unsubscribe;
  onPermissionRequest(cb: (ev: PermissionRequest) => void): Unsubscribe;

  readonly capabilities: { nativeDialogs: boolean };
}
