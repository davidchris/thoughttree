import { create } from 'zustand';
import {
  applyEdgeChanges,
  applyNodeChanges,
  type Connection,
  type Edge,
  type EdgeChange,
  type NodeChange,
} from '@xyflow/react';
import {
  AgentNodeData,
  AgentProvider,
  EffortPreferences,
  FileNodeData,
  ImageAttachment,
  MessageNodeData,
  ModelPreferences,
  ReasoningEffort,
  StoredProviderRecord,
  UserNodeData,
  withoutNullEntries,
} from '../types';
import { useProviderStore } from './useProviderStore';
import { useUIStore } from './useUIStore';
import { computeAutoLayout, type AutoLayoutOptions } from '../lib/graphLayout';
import { logger } from '../lib/logger';
import { getBackendTransport, StaleRevisionError } from '../lib/transport';
import type { AttachmentLimits, BackendTransport, FileStat } from '../lib/transport';
import { isRasterImage } from '../lib/fileNodes';
import {
  GRAPH_JSON_VERSION,
  GraphModel,
  GraphMutations,
  GraphSerialize,
  isFileUrlOrBarePath,
  isWebUrl,
  type FileRef,
  type Graph,
  type GraphJSON,
  type NodeId,
  type TurnActivity,
  type TurnProvenance,
  type TurnReference,
} from '@thoughttree/graph-model';
import { graphToFlowEdges, graphToFlowNodes, type FlowNode } from '../lib/graph/projection';

const COLLAPSED_NODE_HEIGHT = 120;

// Streaming chunks are buffered and applied to the graph at most once per
// interval, so a fast stream doesn't trigger a full graph projection per chunk.
export const STREAM_FLUSH_INTERVAL_MS = 100;

const pendingStreamChunks = new Map<string, string>();
let streamFlushTimer: ReturnType<typeof setTimeout> | null = null;

function scheduleStreamFlush() {
  if (streamFlushTimer !== null) return;
  streamFlushTimer = setTimeout(() => {
    streamFlushTimer = null;
    useGraphStore.getState().flushStreamingChunks();
  }, STREAM_FLUSH_INTERVAL_MS);
}

interface CurrentProjectFile {
  version: typeof GRAPH_JSON_VERSION;
  graph: GraphJSON;
  // On-disk shape: legacy writers stored explicit nulls for unset entries
  projectModelPreferences?: StoredProviderRecord | null;
  projectEffortPreferences?: StoredProviderRecord<ReasoningEffort> | null;
}

// v3 → v4 added Turn provenance (ADR 0006); v4 → v5 added file nodes (ADR 0008).
// Both are no-op migrations on load: the shape is unchanged, older files just
// lack the newer optional node kinds/fields.
interface ProjectFileV3OrV4 extends Omit<CurrentProjectFile, 'version'> {
  version: 3 | 4;
}

interface ProjectFileLegacyV2 {
  version: 1 | 2;
  nodes: Array<{ id: string; position: { x: number; y: number }; [key: string]: unknown }>;
  edges: Array<{ id: string; source: string; target: string; [key: string]: unknown }>;
  // Legacy files predate file nodes, so only text-bearing roles occur here.
  nodeData: Record<string, LegacyV2NodeData>;
  projectModelPreferences?: StoredProviderRecord | null;
}

type LegacyV2NodeData = UserNodeData | AgentNodeData;

type ProjectFile = CurrentProjectFile | ProjectFileV3OrV4 | ProjectFileLegacyV2;

/**
 * What the backend last reported about a file node's Vault file, relative to
 * the mtime/size the node has seen. Transient: never written to the Project file.
 */
export interface FileNodeStatus {
  /** `unavailable`: the stat itself failed (transport or I/O); Reload retries. */
  state: 'ok' | 'changed' | 'missing' | 'invalid' | 'too-large' | 'unavailable';
  stat?: FileStat;
  /** What the backend saw on disk at the last stat; adopted into the node on reload. */
  live?: { mimeType: string; name: string };
}

interface GraphState {
  // Source of truth
  graph: Graph;

  // Derived from graph (kept in sync via projectGraph helper)
  nodes: FlowNode[];
  edges: Edge[];
  nodeData: Map<NodeId, MessageNodeData>;

  // Project state
  projectSession: string;
  projectPath: string | null;
  projectTitle: string | null;
  projectRevision: string | null;
  lastSavedAt: number | null;
  isDirty: boolean;

  // Persisted with the project file, unlike global preferences
  // (see useProviderStore)
  projectModelPreferences: ModelPreferences | null;
  projectEffortPreferences: EffortPreferences | null;

  // Selection and streaming feed the graph projection, so they live here
  // rather than in useUIStore
  selectedNodeId: string | null;
  streamingNodeIds: Set<string>;

  /** Last known on-disk state per file node (see FileNodeStatus). */
  fileNodeStatus: Map<NodeId, FileNodeStatus>;

  // ReactFlow actions
  onNodesChange: (changes: NodeChange[]) => void;
  onEdgesChange: (changes: EdgeChange[]) => void;
  onConnect: (connection: Connection) => void;
  selectNode: (id: string | null) => void;

  // Node actions
  createUserNode: (position?: { x: number; y: number }) => string;
  createAgentNodeDownstream: (parentId: string, provider?: AgentProvider, model?: string) => string;
  createUserNodeDownstream: (parentId: string) => string;
  /** Links a Vault file as a file node (see ADR 0008); the file itself is never copied into the Graph. */
  addFileNode: (
    file: Omit<FileNodeData, 'id' | 'role' | 'content' | 'timestamp'>,
    position: { x: number; y: number },
  ) => string;
  updateNodeContent: (nodeId: string, content: string) => void;
  appendToNode: (nodeId: string, chunk: string) => void;
  flushStreamingChunks: () => void;
  startStreaming: (nodeId: string) => void;
  stopStreaming: (nodeId: string) => void;
  isNodeBlocked: (nodeId: string) => boolean;
  deleteNode: (nodeId: string) => void;

  // Image actions
  addNodeImage: (nodeId: string, image: ImageAttachment) => void;
  removeNodeImage: (nodeId: string, index: number) => void;

  // File node actions
  /** Stats a Vault-relative path and links it as a file node; posts a notice and returns null when it cannot be linked. */
  linkVaultFile: (path: string, position: { x: number; y: number }) => Promise<NodeId | null>;
  /** Cheap stat against the seen mtime/size; updates fileNodeStatus. No-op for other node kinds. */
  refreshFileNodeStat: (nodeId: string) => Promise<void>;
  refreshAllFileNodeStats: () => Promise<void>;
  /** Adopts the current on-disk mtime/size as seen, so a 'changed' node reads as 'ok' again. */
  acknowledgeFileChange: (nodeId: string) => Promise<void>;
  /** The preview refused this image as too large (e.g. longest side over the limit); blocks sending until the file changes. */
  markFileNodeTooLarge: (nodeId: string) => void;
  /** Human reason why a prompt from this user node must not be sent, or null. Walks the Lineage subgraph. */
  sendBlocker: (userNodeId: string) => string | null;
  /** Whether a user node has something to send (text, inline images, or a file node in its lineage) and no sendBlocker. */
  canGenerate: (userNodeId: string) => boolean;

  // Context building
  buildConversationContext: (nodeId: string) => Array<{
    role: string;
    content: string;
    images?: ImageAttachment[];
    files?: FileRef[];
  }>;
  getConversationPathNodeIds: (nodeId: string) => string[];

  // Summary actions
  setSummary: (nodeId: string, summary: string) => void;

  // Model actions (project-scoped; global preferences live in useProviderStore)
  setProjectModelPreferences: (preferences: ModelPreferences | null) => void;
  setProjectModelPreference: (provider: AgentProvider, modelId: string | null) => void;
  getEffectiveModel: (provider: AgentProvider) => string | undefined;
  setProjectEffortPreferences: (preferences: EffortPreferences | null) => void;
  setProjectEffortPreference: (provider: AgentProvider, effort: ReasoningEffort | null) => void;
  getEffectiveEffort: (provider: AgentProvider) => ReasoningEffort | undefined;

  // Project actions
  setProjectPath: (path: string | null) => void;
  projectContent: () => string;
  snapshotProject: () => Promise<string>;
  saveProject: () => Promise<void>;
  saveProjectCopy: () => Promise<void>;
  restoreRecovery: (id: string) => Promise<void>;
  loadProject: (path: string) => Promise<void>;
  newProject: () => void;
  importGraph: (title: string, graph: Graph) => void;
  exportSubgraph: (nodeIds: string[]) => string;

  // Layout actions
  autoLayout: (options?: AutoLayoutOptions) => void;
}

function deserializeProjectFile(data: string) {
  const parsed = JSON.parse(data) as ProjectFile;

  let graph: Graph;
  let projectModelPreferences: ModelPreferences | null;
  let projectEffortPreferences: EffortPreferences | null;

  if (
    (parsed.version === GRAPH_JSON_VERSION || parsed.version === 4 || parsed.version === 3) &&
    'graph' in parsed
  ) {
    graph = GraphSerialize.fromJSON(parsed.graph);
    projectModelPreferences = parsed.projectModelPreferences
      ? withoutNullEntries(parsed.projectModelPreferences)
      : null;
    projectEffortPreferences = parsed.projectEffortPreferences
      ? withoutNullEntries(parsed.projectEffortPreferences)
      : null;
  } else if (parsed.version === 1 || parsed.version === 2) {
    const legacy = parsed as ProjectFileLegacyV2;
    const migratedNodeData = migrateLegacyV2NodeData(legacy.nodeData);
    graph = GraphSerialize.fromLegacyV2({
      version: legacy.version,
      nodes: legacy.nodes,
      edges: legacy.edges,
      nodeData: migratedNodeData,
    });
    projectModelPreferences = legacy.projectModelPreferences
      ? withoutNullEntries(legacy.projectModelPreferences)
      : null;
    projectEffortPreferences = null;
  } else {
    throw new Error(`Unsupported Project file version: ${String(parsed.version)}`);
  }
  return { graph, projectModelPreferences, projectEffortPreferences };
}

function sameEdits(left: GraphState, right: GraphState) {
  return left.projectSession === right.projectSession && left.graph === right.graph &&
    left.projectModelPreferences === right.projectModelPreferences &&
    left.projectEffortPreferences === right.projectEffortPreferences;
}

async function preserveUnsavedWork() {
  useGraphStore.getState().flushStreamingChunks();
  const state = useGraphStore.getState();
  if (state.streamingNodeIds.size > 0) throw new Error('Wait for the current response to finish before replacing the graph.');
  if (!state.isDirty) return state;
  await state.snapshotProject();
  if (!sameEdits(state, useGraphStore.getState())) {
    throw new Error('Edits changed while the recovery snapshot was saved. Try again.');
  }
  return state;
}

let saveQueue: Promise<unknown> = Promise.resolve();
let recoveryRequest = 0;

const generateId = () => crypto.randomUUID();

function debounce<T extends (...args: unknown[]) => unknown>(fn: T, delay: number): T {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  return ((...args: unknown[]) => {
    if (timeoutId) clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  }) as T;
}

// Wraps text in a Markdown code span so it renders verbatim and never autolinks.
function codeSpan(text: string): string {
  const longestRun = Math.max(0, ...Array.from(text.matchAll(/`+/g), (match) => match[0].length));
  const fence = '`'.repeat(longestRun + 1);
  return `${fence} ${text} ${fence}`;
}

// Percent-encodes the characters that would end a Markdown autolink early
// (`<`, `>`, whitespace) so a validated URL yields exactly one link.
function autolink(url: string): string {
  return `<${url.replace(/[<>\s]/g, (char) => encodeURIComponent(char))}>`;
}

// Only http(s) URLs are clickable. file: URLs may carry host paths and are
// redacted; other schemes are emitted as non-clickable text.
function formatUrl(url: string): string {
  if (isWebUrl(url)) return autolink(url);
  if (isFileUrlOrBarePath(url)) return '_(file URL redacted)_';
  return codeSpan(url);
}

function formatReference(reference: TurnReference, index: number): string {
  const relations = reference.relations.join(', ');

  if (reference.type === 'url') {
    const title = reference.title ? `${reference.title} — ` : '';
    return `${index}. **URL:** ${title}${formatUrl(reference.url)} — Relations: ${relations}`;
  }

  if (reference.scope === 'vault') {
    return `${index}. **Vault file:** \`${reference.path}\` — Relations: ${relations}`;
  }

  return `${index}. **External file:** \`${reference.displayName}\` — Relations: ${relations}`;
}

function formatActivity(activity: TurnActivity, index: number): string {
  switch (activity.type) {
    case 'commentary':
      return `${index}. **Commentary:** ${activity.content}`;
    case 'tool':
      return `${index}. **Tool (${activity.kind}, ${activity.status}):** ${activity.title}`;
    case 'unknown':
      return `${index}. **Unknown (${activity.providerType}):** ${activity.label}`;
  }
}

function formatProvenance(provenance: TurnProvenance): string {
  const lines = [
    '### Provenance',
    '',
    `**Completeness:** ${provenance.completeness[0].toUpperCase()}${provenance.completeness.slice(1)}`,
  ];

  if (provenance.references.length > 0) {
    lines.push(
      '',
      '#### References',
      '',
      ...provenance.references.map((reference, index) => formatReference(reference, index + 1)),
    );
  }

  if (provenance.activity.length > 0) {
    lines.push(
      '',
      '#### Turn Activity',
      '',
      ...provenance.activity.map((activity, index) => formatActivity(activity, index + 1)),
    );
  }

  return lines.join('\n');
}

interface ProjectionResult {
  nodes: FlowNode[];
  edges: Edge[];
  nodeData: Map<NodeId, MessageNodeData>;
}

// Attachment limits are constants owned by the core crate; fetch them once per
// transport (a test swaps the transport, which invalidates the cache).
let limitsCache: { transport: BackendTransport; limits: Promise<AttachmentLimits> } | null = null;

function attachmentLimits(): Promise<AttachmentLimits> {
  const transport = getBackendTransport();
  if (limitsCache?.transport !== transport) {
    const limits = transport.getAttachmentLimits().catch((error) => {
      limitsCache = null;
      throw error;
    });
    limitsCache = { transport, limits };
  }
  return limitsCache.limits;
}

function sameStat(a: FileStat, b: FileStat): boolean {
  return a.modifiedEpochMs === b.modifiedEpochMs && a.size === b.size;
}

// Only raster images are sent as bytes and therefore size-limited; every
// other file is a pointer the agent reads itself (epic decisions 4 and 5).
// The mime comes from the backend's live view of the file, not the node: a
// file replaced under the same path may have changed type.
function classifyFileStat(
  node: FileNodeData,
  live: { stat: FileStat; mimeType: string; name: string },
  limits: AttachmentLimits | null,
  previous: FileNodeStatus | undefined,
): FileNodeStatus {
  const { stat, mimeType, name } = live;
  const status = { stat, live: { mimeType, name } };
  if (limits && isRasterImage(mimeType) && stat.size > limits.imageMaxBytes) {
    return { state: 'too-large', ...status };
  }
  // The preview refused this file version (e.g. longest side over the limit,
  // which a stat cannot see). Only a new version on disk clears that.
  if (previous?.state === 'too-large' && (!previous.stat || sameStat(previous.stat, stat))) {
    return { state: 'too-large', ...status };
  }
  const unchanged = stat.modifiedEpochMs === node.seenMtime && stat.size === node.seenSize;
  return { state: unchanged ? 'ok' : 'changed', ...status };
}

function withFileNodeStatus(
  statuses: Map<NodeId, FileNodeStatus>,
  nodeId: NodeId,
  status: FileNodeStatus | null,
): Map<NodeId, FileNodeStatus> {
  const next = new Map(statuses);
  if (status) next.set(nodeId, status);
  else next.delete(nodeId);
  return next;
}

function blockerReason(node: FileNodeData, status: FileNodeStatus | undefined): string | null {
  // Not stat'd yet (project just loaded, node just linked): refuse rather than
  // let a missing or over-limit file reach the backend.
  if (!status) return `"${node.name}" is still being checked. Try again in a moment.`;
  switch (status.state) {
    case 'missing':
      return `"${node.name}" is missing from the notes directory. Restore it or delete its node before sending.`;
    case 'invalid':
      return `"${node.name}" cannot be read from the notes directory. Delete its node before sending.`;
    case 'too-large':
      return `"${node.name}" is too large for the agent. Use a smaller image before sending.`;
    case 'unavailable':
      return `"${node.name}" could not be checked. Reload the file before sending.`;
    default:
      return null;
  }
}

// Recompute the ReactFlow-facing arrays from the canonical Graph value, while
// preserving each node's `measured` dimensions from the prior projection so
// ReactFlow doesn't have to remeasure on every store update.
function projectGraph(
  graph: Graph,
  prevNodes: FlowNode[],
  selectedNodeId: NodeId | null,
): ProjectionResult {
  const projected = graphToFlowNodes(graph, { selectedNodeId });
  const prevById = new Map(prevNodes.map((n) => [n.id, n]));
  const nodes = projected.map((n) => {
    const prev = prevById.get(n.id);
    if (!prev) return n;
    return { ...n, measured: prev.measured, width: prev.width, height: prev.height } as FlowNode;
  });
  return {
    nodes,
    edges: graphToFlowEdges(graph),
    nodeData: graph.nodes,
  };
}

function migrateLegacyV2NodeData(
  raw: Record<string, LegacyV2NodeData>,
): Record<string, LegacyV2NodeData> {
  const migrated: Record<string, LegacyV2NodeData> = {};
  for (const [id, node] of Object.entries(raw)) {
    const contentUpdatedAt = node.contentUpdatedAt ?? node.timestamp;
    if (node.role === 'assistant' && !('provider' in node)) {
      // Files predating provider selection were generated by Claude.
      migrated[id] = { ...node, contentUpdatedAt, provider: 'claude-code' } as AgentNodeData;
    } else {
      migrated[id] = { ...node, contentUpdatedAt };
    }
  }
  return migrated;
}

function serializeProjectFile(
  graph: Graph,
  projectModelPreferences: ModelPreferences | null,
  projectEffortPreferences: EffortPreferences | null
): string {
  const projectFile: CurrentProjectFile = {
    version: GRAPH_JSON_VERSION,
    graph: GraphSerialize.toJSON(graph),
    projectModelPreferences,
    projectEffortPreferences,
  };
  return JSON.stringify(projectFile, null, 2);
}

export const useGraphStore = create<GraphState>()((set, get) => ({
  graph: GraphMutations.empty(),
  nodes: [],
  edges: [],
  nodeData: new Map(),
  projectSession: crypto.randomUUID(),
  projectPath: null,
  projectTitle: null,
  projectRevision: null,
  lastSavedAt: null,
  isDirty: false,
  projectModelPreferences: null,
  projectEffortPreferences: null,
  selectedNodeId: null,
  streamingNodeIds: new Set<string>(),
  fileNodeStatus: new Map(),

  onNodesChange: (changes) => {
    const state = get();
    const newNodes = applyNodeChanges(changes, state.nodes) as FlowNode[];
    let graph = state.graph;
    let dirty = state.isDirty;
    let selectedNodeId = state.selectedNodeId;
    let streamingNodeIds = state.streamingNodeIds;
    let streamingMutated = false;
    let fileNodeStatus = state.fileNodeStatus;

    // Copy-on-write removal so the streaming set is only cloned when it changes
    const stopStreaming = (nodeId: string) => {
      if (!streamingNodeIds.has(nodeId)) return;
      if (!streamingMutated) {
        streamingNodeIds = new Set(streamingNodeIds);
        streamingMutated = true;
      }
      streamingNodeIds.delete(nodeId);
    };

    for (const change of changes) {
      if (change.type === 'position' && change.position && change.dragging === false) {
        graph = GraphMutations.setPosition(graph, change.id, change.position);
        dirty = true;
      } else if (change.type === 'remove') {
        graph = GraphMutations.removeNode(graph, change.id);
        dirty = true;
        if (selectedNodeId === change.id) selectedNodeId = null;
        useUIStore.getState().clearNodeRefs(change.id);
        if (fileNodeStatus.has(change.id)) {
          fileNodeStatus = withFileNodeStatus(fileNodeStatus, change.id, null);
        }
        stopStreaming(change.id);
      } else if (change.type !== 'select' && change.type !== 'dimensions') {
        dirty = true;
      }
    }

    set({
      nodes: newNodes,
      graph,
      edges: graphToFlowEdges(graph),
      nodeData: graph.nodes,
      isDirty: dirty,
      selectedNodeId,
      streamingNodeIds,
      fileNodeStatus,
    });
  },

  onEdgesChange: (changes) => {
    const state = get();
    const newEdges = applyEdgeChanges(changes, state.edges);
    let graph = state.graph;
    let dirty = state.isDirty;

    for (const change of changes) {
      if (change.type === 'remove') {
        graph = { ...graph, edges: graph.edges.filter((e) => e.id !== change.id) };
        dirty = true;
      } else if (change.type !== 'select') {
        dirty = true;
      }
    }

    set({
      edges: newEdges,
      graph,
      nodeData: graph.nodes,
      isDirty: dirty,
    });
  },

  onConnect: (connection) => {
    if (!connection.source || !connection.target) return;
    const state = get();
    const graph = GraphMutations.addEdge(state.graph, connection.source, connection.target);
    set({
      graph,
      nodes: state.nodes,
      edges: graphToFlowEdges(graph),
      nodeData: graph.nodes,
      isDirty: true,
    });
  },

  selectNode: (id) => {
    const state = get();
    set({
      selectedNodeId: id,
      ...projectGraph(state.graph, state.nodes, id),
    });
  },

  createUserNode: (position = { x: 100, y: 100 }) => {
    const id = generateId();
    const data: UserNodeData = {
      id,
      role: 'user',
      content: '',
      timestamp: Date.now(),
      contentUpdatedAt: Date.now(),
    };
    const state = get();
    const graph = GraphMutations.addNode(state.graph, data, position);
    set({
      graph,
      ...projectGraph(graph, state.nodes, id),
      selectedNodeId: id,
      isDirty: true,
    });
    useUIStore.getState().setEditing(id);
    return id;
  },

  createAgentNodeDownstream: (parentId, provider, model) => {
    const id = generateId();
    const state = get();
    const activeProvider = provider ?? useProviderStore.getState().defaultProvider;
    const activeModel = model ?? state.getEffectiveModel(activeProvider);
    const data: AgentNodeData = {
      id,
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      contentUpdatedAt: Date.now(),
      provider: activeProvider,
      model: activeModel,
    };
    const parentPos = state.graph.layout.get(parentId);
    const position = parentPos
      ? { x: parentPos.x, y: parentPos.y + COLLAPSED_NODE_HEIGHT }
      : { x: 100, y: 100 };

    let graph = GraphMutations.addNode(state.graph, data, position);
    graph = GraphMutations.addEdge(graph, parentId, id);

    const streamingNodeIds = new Set(state.streamingNodeIds);
    streamingNodeIds.add(id);

    set({
      graph,
      ...projectGraph(graph, state.nodes, id),
      selectedNodeId: id,
      streamingNodeIds,
      isDirty: true,
    });
    return id;
  },

  createUserNodeDownstream: (parentId) => {
    const id = generateId();
    const state = get();
    const data: UserNodeData = {
      id,
      role: 'user',
      content: '',
      timestamp: Date.now(),
      contentUpdatedAt: Date.now(),
    };
    const parentPos = state.graph.layout.get(parentId);
    const position = parentPos
      ? { x: parentPos.x, y: parentPos.y + COLLAPSED_NODE_HEIGHT }
      : { x: 100, y: 100 };

    let graph = GraphMutations.addNode(state.graph, data, position);
    graph = GraphMutations.addEdge(graph, parentId, id);

    set({
      graph,
      ...projectGraph(graph, state.nodes, id),
      selectedNodeId: id,
      isDirty: true,
    });
    useUIStore.getState().setEditing(id);
    return id;
  },

  addFileNode: (file, position) => {
    const id = generateId();
    const data: FileNodeData = { ...file, id, role: 'file', content: '', timestamp: Date.now() };
    const state = get();
    const graph = GraphMutations.addNode(state.graph, data, position);
    set({
      graph,
      ...projectGraph(graph, state.nodes, id),
      selectedNodeId: id,
      isDirty: true,
    });
    return id;
  },

  updateNodeContent: (nodeId, content) => {
    const state = get();
    const graph = GraphMutations.updateNode(state.graph, nodeId, {
      content,
      contentUpdatedAt: Date.now(),
    });
    if (graph === state.graph) return;
    set({
      graph,
      ...projectGraph(graph, state.nodes, state.selectedNodeId),
      isDirty: true,
    });
  },

  appendToNode: (nodeId, chunk) => {
    pendingStreamChunks.set(nodeId, (pendingStreamChunks.get(nodeId) ?? '') + chunk);
    scheduleStreamFlush();
  },

  flushStreamingChunks: () => {
    if (pendingStreamChunks.size === 0) return;
    const state = get();
    const now = Date.now();
    let graph = state.graph;
    for (const [nodeId, text] of pendingStreamChunks) {
      graph = GraphMutations.appendContent(graph, nodeId, text, now);
    }
    pendingStreamChunks.clear();
    if (graph === state.graph) return;
    set({
      graph,
      ...projectGraph(graph, state.nodes, state.selectedNodeId),
      isDirty: true,
    });
  },

  startStreaming: (nodeId) => {
    logger.debug('[Store] startStreaming called with:', nodeId);
    set((state) => {
      const next = new Set(state.streamingNodeIds);
      next.add(nodeId);
      return { streamingNodeIds: next };
    });
  },

  stopStreaming: (nodeId) => {
    logger.debug('[Store] stopStreaming called with:', nodeId);
    get().flushStreamingChunks();
    set((state) => {
      const next = new Set(state.streamingNodeIds);
      next.delete(nodeId);
      return { streamingNodeIds: next };
    });
  },

  isNodeBlocked: (nodeId) => {
    const { graph, streamingNodeIds } = get();
    if (streamingNodeIds.size === 0) return false;
    if (streamingNodeIds.has(nodeId)) return true;
    const ancs = GraphModel.ancestors(graph, nodeId);
    const desc = GraphModel.descendants(graph, nodeId);
    for (const id of streamingNodeIds) {
      if (ancs.has(id) || desc.has(id)) return true;
    }
    return false;
  },

  deleteNode: (nodeId) => {
    pendingStreamChunks.delete(nodeId);
    const state = get();
    const graph = GraphMutations.removeNode(state.graph, nodeId);
    if (graph === state.graph) return;

    const streamingNodeIds = new Set(state.streamingNodeIds);
    streamingNodeIds.delete(nodeId);

    const selectedNodeId = state.selectedNodeId === nodeId ? null : state.selectedNodeId;

    set({
      graph,
      ...projectGraph(graph, state.nodes, selectedNodeId),
      streamingNodeIds,
      selectedNodeId,
      fileNodeStatus: withFileNodeStatus(state.fileNodeStatus, nodeId, null),
      isDirty: true,
    });
    useUIStore.getState().clearNodeRefs(nodeId);
  },

  addNodeImage: (nodeId, image) => {
    const state = get();
    const node = state.graph.nodes.get(nodeId);
    if (!node || node.role !== 'user') return;
    const userNode = node as UserNodeData;
    const updated: UserNodeData = {
      ...userNode,
      images: [...(userNode.images ?? []), image],
    };
    const graph = GraphMutations.updateNode(state.graph, nodeId, updated);
    set({
      graph,
      ...projectGraph(graph, state.nodes, state.selectedNodeId),
      isDirty: true,
    });
  },

  removeNodeImage: (nodeId, index) => {
    const state = get();
    const node = state.graph.nodes.get(nodeId);
    if (!node || node.role !== 'user') return;
    const userNode = node as UserNodeData;
    if (!userNode.images || index >= userNode.images.length) return;
    const updated: UserNodeData = {
      ...userNode,
      images: userNode.images.filter((_, i) => i !== index),
    };
    const graph = GraphMutations.updateNode(state.graph, nodeId, updated);
    set({
      graph,
      ...projectGraph(graph, state.nodes, state.selectedNodeId),
      isDirty: true,
    });
  },

  linkVaultFile: async (path, position) => {
    const status = await getBackendTransport().statVaultFile(path);
    if (status.status !== 'ok') {
      const why =
        status.status === 'missing' ? 'was not found in the notes directory' : 'is not a readable file in the notes directory';
      useUIStore.getState().setNotice(`"${path}" ${why}.`);
      return null;
    }
    const id = get().addFileNode(
      {
        path,
        name: status.name,
        mimeType: status.mimeType,
        size: status.stat.size,
        seenMtime: status.stat.modifiedEpochMs,
        seenSize: status.stat.size,
      },
      position,
    );
    await get().refreshFileNodeStat(id);
    return id;
  },

  refreshFileNodeStat: async (nodeId) => {
    const node = get().graph.nodes.get(nodeId);
    if (node?.role !== 'file') return;
    try {
      const status = await getBackendTransport().statVaultFile(node.path);
      // Limits matter only for raster images, judged by the live mime.
      const limits =
        status.status === 'ok' && isRasterImage(status.mimeType)
          ? await attachmentLimits().catch((error) => {
              logger.warn('Attachment limits unavailable; skipping image size check:', error);
              return null;
            })
          : null;
      // The node may have been deleted or the project swapped while we waited.
      const current = get().graph.nodes.get(nodeId);
      if (current?.role !== 'file' || current.path !== node.path) return;
      set((state) => {
        const next: FileNodeStatus =
          status.status === 'ok'
            ? classifyFileStat(current, status, limits, state.fileNodeStatus.get(nodeId))
            : { state: status.status };
        return { fileNodeStatus: withFileNodeStatus(state.fileNodeStatus, nodeId, next) };
      });
    } catch (error) {
      logger.error('Failed to stat file node:', error);
      // Record the failure so the node is not stuck as "still being checked".
      set((state) =>
        state.graph.nodes.get(nodeId)?.role === 'file'
          ? { fileNodeStatus: withFileNodeStatus(state.fileNodeStatus, nodeId, { state: 'unavailable' }) }
          : {}
      );
    }
  },

  refreshAllFileNodeStats: async () => {
    const ids: NodeId[] = [];
    for (const node of get().graph.nodes.values()) {
      if (node.role === 'file') ids.push(node.id);
    }
    await Promise.all(ids.map((id) => get().refreshFileNodeStat(id)));
  },

  acknowledgeFileChange: async (nodeId) => {
    // Always re-stat: "reload" means the version on disk right now, not the
    // one a focus event happened to observe earlier.
    await get().refreshFileNodeStat(nodeId);
    const state = get();
    const node = state.graph.nodes.get(nodeId);
    const status = state.fileNodeStatus.get(nodeId);
    const stat = status?.stat;
    const live = status?.live;
    if (node?.role !== 'file' || !stat || !live) return;
    // Adopt everything the backend saw: a file replaced under the same path
    // may have changed type, and the node must describe what will be sent.
    const updated: FileNodeData = {
      ...node,
      name: live.name,
      mimeType: live.mimeType,
      size: stat.size,
      seenMtime: stat.modifiedEpochMs,
      seenSize: stat.size,
    };
    const graph = GraphMutations.updateNode(state.graph, nodeId, updated);
    const limits = isRasterImage(live.mimeType) ? await attachmentLimits().catch(() => null) : null;
    set({
      graph,
      ...projectGraph(graph, state.nodes, state.selectedNodeId),
      fileNodeStatus: withFileNodeStatus(
        state.fileNodeStatus,
        nodeId,
        classifyFileStat(updated, { stat, ...live }, limits, status),
      ),
      isDirty: true,
    });
  },

  markFileNodeTooLarge: (nodeId) => {
    const state = get();
    if (state.graph.nodes.get(nodeId)?.role !== 'file') return;
    const previous = state.fileNodeStatus.get(nodeId);
    if (previous?.state === 'too-large') return;
    const next: FileNodeStatus = previous?.stat ? { state: 'too-large', stat: previous.stat } : { state: 'too-large' };
    set({ fileNodeStatus: withFileNodeStatus(state.fileNodeStatus, nodeId, next) });
  },

  sendBlocker: (userNodeId) => {
    const { graph, fileNodeStatus } = get();
    for (const id of GraphModel.conversationPathIds(graph, userNodeId)) {
      const node = graph.nodes.get(id);
      if (node?.role !== 'file') continue;
      const reason = blockerReason(node, fileNodeStatus.get(id));
      if (reason) return reason;
    }
    return null;
  },

  canGenerate: (userNodeId) => {
    const { graph } = get();
    const node = graph.nodes.get(userNodeId);
    if (node?.role !== 'user') return false;
    // A file node in the lineage stands in for text: the backend sends the
    // file-only placeholder when the merged user message has no text (decision 3).
    const hasFileAncestor = () => {
      for (const id of GraphModel.ancestors(graph, userNodeId)) {
        if (graph.nodes.get(id)?.role === 'file') return true;
      }
      return false;
    };
    const hasInput = !!node.content.trim() || (node.images?.length ?? 0) > 0 || hasFileAncestor();
    return hasInput && get().sendBlocker(userNodeId) === null;
  },

  buildConversationContext: (nodeId) => {
    get().flushStreamingChunks();
    return GraphModel.conversationPath(get().graph, nodeId);
  },
  getConversationPathNodeIds: (nodeId) => GraphModel.conversationPathIds(get().graph, nodeId),

  setSummary: (nodeId, summary) => {
    const state = get();
    const graph = GraphMutations.updateNode(state.graph, nodeId, {
      summary,
      summaryTimestamp: Date.now(),
    });
    if (graph === state.graph) return;
    set({ graph, ...projectGraph(graph, state.nodes, state.selectedNodeId) });
  },

  setProjectModelPreferences: (preferences) => set({ projectModelPreferences: preferences }),

  setProjectModelPreference: (provider, modelId) => {
    set((state) => ({
      projectModelPreferences: {
        ...(state.projectModelPreferences ?? {}),
        [provider]: modelId ?? undefined,
      },
      isDirty: true,
    }));
  },

  getEffectiveModel: (provider) => {
    const { projectModelPreferences } = get();
    const { globalModelPreferences } = useProviderStore.getState();
    return projectModelPreferences?.[provider] ?? globalModelPreferences[provider];
  },

  setProjectEffortPreferences: (preferences) => set({ projectEffortPreferences: preferences }),

  setProjectEffortPreference: (provider, effort) => {
    set((state) => ({
      projectEffortPreferences: {
        ...(state.projectEffortPreferences ?? {}),
        [provider]: effort ?? undefined,
      },
      isDirty: true,
    }));
  },

  getEffectiveEffort: (provider) => {
    const { projectEffortPreferences } = get();
    const { globalEffortPreferences } = useProviderStore.getState();
    return projectEffortPreferences?.[provider] ?? globalEffortPreferences[provider];
  },

  setProjectPath: (path) =>
    set((state) => ({
      projectSession: state.projectPath === path ? state.projectSession : crypto.randomUUID(),
      projectPath: path,
      projectTitle: path ? null : state.projectTitle,
      projectRevision: state.projectPath === path ? state.projectRevision : null,
    })),

  projectContent: () => {
    get().flushStreamingChunks();
    const { graph, projectModelPreferences, projectEffortPreferences } = get();
    return serializeProjectFile(graph, projectModelPreferences, projectEffortPreferences);
  },

  snapshotProject: async () => {
    const content = get().projectContent();
    const session = get().projectSession;
    const request = ++recoveryRequest;
    try {
      const id = await getBackendTransport().snapshotProject(get().projectPath, content);
      if (request === recoveryRequest && get().projectSession === session) useUIStore.getState().setRecoveryError(null);
      return id;
    } catch (error) {
      if (request === recoveryRequest && get().projectSession === session) {
        useUIStore.getState().setRecoveryError('Recovery snapshot failed. Keep this window open and save a separate copy.');
      }
      throw error;
    }
  },

  saveProject: () => {
    const session = get().projectSession;
    const operation = saveQueue.then(async () => {
      if (get().projectSession !== session) return;
      const content = get().projectContent();
      const state = get();
      if (!state.projectPath) return;
      const conflict = useUIStore.getState().staleProjectSave;
      if (conflict?.path === state.projectPath) throw new StaleRevisionError(conflict.currentRevision);
      try {
        const revision = await getBackendTransport().saveProject(state.projectPath, content, state.projectRevision);
        if (get().projectSession !== session) return;
        set({ projectRevision: revision, lastSavedAt: Date.now(), isDirty: !sameEdits(state, get()) });
        useUIStore.getState().setStaleProjectSave(null);
      } catch (error) {
        if (get().projectSession === session && error instanceof StaleRevisionError) {
          set({ isDirty: true });
          useUIStore.getState().setStaleProjectSave({ path: state.projectPath, currentRevision: error.currentRevision });
        }
        throw error;
      }
    });
    saveQueue = operation.catch(() => {});
    return operation;
  },

  saveProjectCopy: async () => {
    const content = get().projectContent();
    const state = get();
    if (!state.projectPath) throw new Error('Choose a project location before saving a copy.');
    const [path, revision] = await getBackendTransport().saveProjectCopy(state.projectPath, content);
    if (get().projectSession !== state.projectSession) return;
    set({ projectPath: path, projectRevision: revision, projectSession: crypto.randomUUID(),
      lastSavedAt: Date.now(), isDirty: !sameEdits(state, get()) });
    useUIStore.getState().setStaleProjectSave(null);
  },

  restoreRecovery: async (id) => {
    const session = get().projectSession;
    const content = await getBackendTransport().readProjectRecovery(id);
    const { graph, projectModelPreferences, projectEffortPreferences } = deserializeProjectFile(content);
    const preserved = await preserveUnsavedWork();
    if (!sameEdits(preserved, get())) throw new Error('Edits changed during recovery. Try again.');
    if (get().projectSession !== session) throw new Error('The active project changed during recovery.');
    get().importGraph('Recovered project', graph);
    set({ projectModelPreferences, projectEffortPreferences });
  },

  loadProject: async (path) => {
    const session = get().projectSession;
    const transport = getBackendTransport();
    try {
      const project = await transport.loadProject(path);
      const { graph, projectModelPreferences, projectEffortPreferences } = deserializeProjectFile(project.data);
      const preserved = await preserveUnsavedWork();
      if (!sameEdits(preserved, get())) throw new Error('Edits changed during reload. Try again.');
      if (get().projectSession !== session) throw new Error('The active project changed during reload.');

      set({
        graph,
        ...projectGraph(graph, [], null),
        projectModelPreferences,
        projectEffortPreferences,
        projectSession: crypto.randomUUID(),
        projectPath: path,
        projectTitle: null,
        projectRevision: project.revision,
        lastSavedAt: Date.now(),
        isDirty: false,
        selectedNodeId: null,
        streamingNodeIds: new Set<string>(),
        fileNodeStatus: new Map(),
      });
      useUIStore.getState().reset();
      void get().refreshAllFileNodeStats();

      logger.info('Project loaded from:', path);
    } catch (error) {
      logger.error('Failed to load project:', error);
      throw error;
    }
  },

  newProject: () => {
    const graph = GraphMutations.empty();
    set({
      graph,
      nodes: [],
      edges: [],
      nodeData: graph.nodes,
      projectModelPreferences: null,
      projectEffortPreferences: null,
      projectSession: crypto.randomUUID(),
      projectPath: null,
      projectTitle: null,
      projectRevision: null,
      lastSavedAt: null,
      isDirty: false,
      selectedNodeId: null,
      streamingNodeIds: new Set<string>(),
      fileNodeStatus: new Map(),
    });
    useUIStore.getState().reset();
  },

  importGraph: (title, graph) => {
    set({
      graph,
      ...projectGraph(graph, [], null),
      projectModelPreferences: null,
      projectEffortPreferences: null,
      projectSession: crypto.randomUUID(),
      projectPath: null,
      projectTitle: title,
      projectRevision: null,
      lastSavedAt: null,
      isDirty: true,
      selectedNodeId: null,
      streamingNodeIds: new Set<string>(),
      fileNodeStatus: new Map(),
    });
    useUIStore.getState().reset();
    void get().refreshAllFileNodeStats();
  },

  exportSubgraph: (nodeIds) => {
    const { graph } = get();
    const nodeSet = new Set(nodeIds);

    const hasIncoming = new Set<NodeId>();
    for (const e of graph.edges) {
      if (nodeSet.has(e.source) && nodeSet.has(e.target)) {
        hasIncoming.add(e.target);
      }
    }

    const startNode = nodeIds.find((id) => !hasIncoming.has(id)) ?? nodeIds[0];

    const ordered: NodeId[] = [];
    const visited = new Set<NodeId>();
    let current: NodeId | undefined = startNode;
    while (current && !visited.has(current) && nodeSet.has(current)) {
      visited.add(current);
      ordered.push(current);
      const nextEdge = graph.edges.find((e) => e.source === current && nodeSet.has(e.target));
      current = nextEdge?.target;
    }

    return ordered
      .map((id) => {
        const node = graph.nodes.get(id);
        if (!node) return '';
        if (node.role === 'file') return `## File\n\n${node.path}`;
        const header = node.role === 'user' ? '## User' : '## Assistant';
        const provenance =
          node.role === 'assistant' && node.provenance
            ? `\n\n${formatProvenance(node.provenance)}`
            : '';
        return `${header}\n\n${node.content}${provenance}`;
      })
      .filter(Boolean)
      .join('\n\n---\n\n');
  },

  autoLayout: (options) => {
    const state = get();
    if (state.graph.nodes.size === 0) return;

    const positions = computeAutoLayout(state.nodes, state.edges, options);
    let graph = state.graph;
    for (const [id, p] of positions) {
      graph = GraphMutations.setPosition(graph, id, p);
    }

    set({
      graph,
      ...projectGraph(graph, state.nodes, state.selectedNodeId),
      isDirty: true,
    });
  },
}));

// Auto-save subscription: graph reference changes whenever domain content mutates.
const debouncedSave = debounce(async () => {
  const state = useGraphStore.getState();
  if (state.projectPath && state.isDirty && !useUIStore.getState().staleProjectSave) {
    try {
      await state.saveProject();
    } catch (error) {
      logger.error('Auto-save failed:', error);
    }
  }
}, 2000);

// At most one draft checkpoint per second, including continuous edits/streaming.
// This timer is not reset by each edit, so a long stream cannot starve recovery.
let recoveryTimer: ReturnType<typeof setTimeout> | null = null;
useGraphStore.subscribe((state, prevState) => {
  if (!sameEdits(state, prevState)) {
    void debouncedSave();
    if (state.isDirty && recoveryTimer === null) {
      recoveryTimer = setTimeout(() => {
        recoveryTimer = null;
        const current = useGraphStore.getState();
        if (current.isDirty) void current.snapshotProject().catch((error) => logger.error('Recovery snapshot failed:', error));
      }, 1000);
    }
  }
});
