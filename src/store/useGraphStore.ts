import { create } from 'zustand';
import {
  applyEdgeChanges,
  applyNodeChanges,
  type Connection,
  type Edge,
  type EdgeChange,
  type NodeChange,
} from '@xyflow/react';
import { invoke } from '@tauri-apps/api/core';
import {
  AgentNodeData,
  AgentProvider,
  DEFAULT_PROVIDER,
  ImageAttachment,
  MessageNodeData,
  ModelInfo,
  ModelPreferences,
  PermissionRequest,
  ProviderStatus,
  UserNodeData,
} from '../types';
import { computeAutoLayout, type AutoLayoutOptions } from '../lib/graphLayout';
import { logger } from '../lib/logger';
import {
  GRAPH_JSON_VERSION,
  GraphModel,
  GraphMutations,
  GraphSerialize,
  graphToFlowEdges,
  graphToFlowNodes,
  type FlowNode,
  type Graph,
  type GraphJSON,
  type NodeId,
} from '../lib/graph';

const COLLAPSED_NODE_HEIGHT = 120;

interface ProjectFileV3 {
  version: 3;
  graph: GraphJSON;
  projectModelPreferences?: ModelPreferences | null;
}

interface ProjectFileLegacyV2 {
  version: number;
  nodes: Array<{ id: string; position: { x: number; y: number }; [key: string]: unknown }>;
  edges: Array<{ id: string; source: string; target: string; [key: string]: unknown }>;
  nodeData: Record<string, MessageNodeData>;
  projectModelPreferences?: ModelPreferences | null;
}

type ProjectFile = ProjectFileV3 | ProjectFileLegacyV2;

interface GraphState {
  // Source of truth
  graph: Graph;

  // Derived from graph (kept in sync via projectGraph helper)
  nodes: FlowNode[];
  edges: Edge[];
  nodeData: Map<NodeId, MessageNodeData>;

  // Project state
  projectPath: string | null;
  lastSavedAt: number | null;
  isDirty: boolean;

  // Provider state
  defaultProvider: AgentProvider;
  availableProviders: ProviderStatus[];

  // Model state
  globalModelPreferences: ModelPreferences;
  projectModelPreferences: ModelPreferences | null;
  availableModels: Record<AgentProvider, ModelInfo[]>;

  // UI state
  selectedNodeId: string | null;
  streamingNodeIds: Set<string>;
  editingNodeId: string | null;
  previewNodeId: string | null;
  pendingPermission: PermissionRequest | null;
  triggerSidePanelEdit: boolean;

  // ReactFlow actions
  onNodesChange: (changes: NodeChange[]) => void;
  onEdgesChange: (changes: EdgeChange[]) => void;
  onConnect: (connection: Connection) => void;
  selectNode: (id: string | null) => void;

  // Node actions
  createUserNode: (position?: { x: number; y: number }) => string;
  createAgentNodeDownstream: (parentId: string, provider?: AgentProvider, model?: string) => string;
  createUserNodeDownstream: (parentId: string) => string;
  updateNodeContent: (nodeId: string, content: string) => void;
  appendToNode: (nodeId: string, chunk: string) => void;
  startStreaming: (nodeId: string) => void;
  stopStreaming: (nodeId: string) => void;
  isNodeBlocked: (nodeId: string) => boolean;
  setEditing: (nodeId: string | null) => void;
  setPreviewNode: (nodeId: string | null) => void;
  togglePreviewNode: (nodeId: string) => void;
  deleteNode: (nodeId: string) => void;
  triggerSidePanelEditMode: () => void;
  clearSidePanelEditTrigger: () => void;

  // Image actions
  addNodeImage: (nodeId: string, image: ImageAttachment) => void;
  removeNodeImage: (nodeId: string, index: number) => void;

  // Context building
  buildConversationContext: (nodeId: string) => Array<{
    role: string;
    content: string;
    images?: ImageAttachment[];
  }>;
  getConversationPathNodeIds: (nodeId: string) => string[];

  // Summary actions
  setSummary: (nodeId: string, summary: string) => void;

  // Permission actions
  setPendingPermission: (permission: PermissionRequest | null) => void;

  // Provider actions
  setDefaultProvider: (provider: AgentProvider) => void;
  setAvailableProviders: (providers: ProviderStatus[]) => void;

  // Model actions
  setGlobalModelPreferences: (preferences: ModelPreferences) => void;
  setGlobalModelPreference: (provider: AgentProvider, modelId: string | null) => void;
  setProjectModelPreferences: (preferences: ModelPreferences | null) => void;
  setProjectModelPreference: (provider: AgentProvider, modelId: string | null) => void;
  setAvailableModels: (provider: AgentProvider, models: ModelInfo[]) => void;
  getEffectiveModel: (provider: AgentProvider) => string | undefined;

  // Project actions
  setProjectPath: (path: string | null) => void;
  saveProject: () => Promise<void>;
  loadProject: (path: string) => Promise<void>;
  newProject: () => void;
  exportSubgraph: (nodeIds: string[]) => string;

  // Layout actions
  autoLayout: (options?: AutoLayoutOptions) => void;
}

const generateId = () => crypto.randomUUID();

function debounce<T extends (...args: unknown[]) => unknown>(fn: T, delay: number): T {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  return ((...args: unknown[]) => {
    if (timeoutId) clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  }) as T;
}

interface ProjectionResult {
  nodes: FlowNode[];
  edges: Edge[];
  nodeData: Map<NodeId, MessageNodeData>;
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
  raw: Record<string, MessageNodeData>,
): Record<string, MessageNodeData> {
  const migrated: Record<string, MessageNodeData> = {};
  for (const [id, node] of Object.entries(raw)) {
    const contentUpdatedAt = node.contentUpdatedAt ?? node.timestamp;
    if (node.role === 'assistant' && !('provider' in node)) {
      migrated[id] = { ...node, contentUpdatedAt, provider: DEFAULT_PROVIDER } as AgentNodeData;
    } else {
      migrated[id] = { ...node, contentUpdatedAt };
    }
  }
  return migrated;
}

export const useGraphStore = create<GraphState>()((set, get) => ({
  graph: GraphMutations.empty(),
  nodes: [],
  edges: [],
  nodeData: new Map(),
  projectPath: null,
  lastSavedAt: null,
  isDirty: false,
  defaultProvider: DEFAULT_PROVIDER,
  availableProviders: [],
  globalModelPreferences: {},
  projectModelPreferences: null,
  availableModels: {} as Record<AgentProvider, ModelInfo[]>,
  selectedNodeId: null,
  streamingNodeIds: new Set<string>(),
  editingNodeId: null,
  previewNodeId: null,
  pendingPermission: null,
  triggerSidePanelEdit: false,

  onNodesChange: (changes) => {
    const state = get();
    const newNodes = applyNodeChanges(changes, state.nodes) as FlowNode[];
    let graph = state.graph;
    let dirty = state.isDirty;

    for (const change of changes) {
      if (change.type === 'position' && change.position && change.dragging === false) {
        graph = GraphMutations.setPosition(graph, change.id, change.position);
        dirty = true;
      } else if (change.type === 'remove') {
        graph = GraphMutations.removeNode(graph, change.id);
        dirty = true;
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
      editingNodeId: id,
      isDirty: true,
    });
    return id;
  },

  createAgentNodeDownstream: (parentId, provider, model) => {
    const id = generateId();
    const state = get();
    const activeProvider = provider ?? state.defaultProvider;
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
      editingNodeId: id,
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
    const state = get();
    const graph = GraphMutations.appendContent(state.graph, nodeId, chunk, Date.now());
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

  setEditing: (nodeId) => set({ editingNodeId: nodeId }),
  setPreviewNode: (nodeId) => set({ previewNodeId: nodeId }),
  togglePreviewNode: (nodeId) =>
    set((state) => ({ previewNodeId: state.previewNodeId === nodeId ? null : nodeId })),

  deleteNode: (nodeId) => {
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
      editingNodeId: state.editingNodeId === nodeId ? null : state.editingNodeId,
      previewNodeId: state.previewNodeId === nodeId ? null : state.previewNodeId,
      isDirty: true,
    });
  },

  triggerSidePanelEditMode: () => set({ triggerSidePanelEdit: true }),
  clearSidePanelEditTrigger: () => set({ triggerSidePanelEdit: false }),

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

  buildConversationContext: (nodeId) => GraphModel.conversationPath(get().graph, nodeId),
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

  setPendingPermission: (permission) => set({ pendingPermission: permission }),

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

  setAvailableModels: (provider, models) => {
    set((state) => ({
      availableModels: { ...state.availableModels, [provider]: models },
    }));
  },

  getEffectiveModel: (provider) => {
    const { projectModelPreferences, globalModelPreferences } = get();
    return projectModelPreferences?.[provider] ?? globalModelPreferences[provider];
  },

  setProjectPath: (path) => set({ projectPath: path }),

  saveProject: async () => {
    const { projectPath, graph, projectModelPreferences } = get();
    if (!projectPath) {
      logger.warn('No project path set, cannot save');
      return;
    }

    const projectFile: ProjectFileV3 = {
      version: GRAPH_JSON_VERSION,
      graph: GraphSerialize.toJSON(graph),
      projectModelPreferences,
    };

    try {
      await invoke('save_project', {
        path: projectPath,
        data: JSON.stringify(projectFile, null, 2),
      });
      set({ lastSavedAt: Date.now(), isDirty: false });
      logger.info('Project saved to:', projectPath);
    } catch (error) {
      logger.error('Failed to save project:', error);
      throw error;
    }
  },

  loadProject: async (path) => {
    try {
      const data = await invoke<string>('load_project', { path });
      const parsed = JSON.parse(data) as ProjectFile;

      let graph: Graph;
      let projectModelPreferences: ModelPreferences | null = null;

      if (parsed.version === GRAPH_JSON_VERSION && 'graph' in parsed) {
        graph = GraphSerialize.fromJSON(parsed.graph);
        projectModelPreferences = parsed.projectModelPreferences ?? null;
      } else {
        const legacy = parsed as ProjectFileLegacyV2;
        const migratedNodeData = migrateLegacyV2NodeData(legacy.nodeData);
        graph = GraphSerialize.fromLegacyV2({
          version: legacy.version,
          nodes: legacy.nodes,
          edges: legacy.edges,
          nodeData: migratedNodeData,
        });
        projectModelPreferences = legacy.projectModelPreferences ?? null;
      }

      set({
        graph,
        ...projectGraph(graph, [], null),
        projectModelPreferences,
        projectPath: path,
        lastSavedAt: Date.now(),
        isDirty: false,
        selectedNodeId: null,
        editingNodeId: null,
        streamingNodeIds: new Set<string>(),
        previewNodeId: null,
      });

      try {
        await invoke('add_recent_project', { path });
      } catch (error) {
        logger.warn('Failed to update recent projects:', error);
      }

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
      projectPath: null,
      lastSavedAt: null,
      isDirty: false,
      selectedNodeId: null,
      editingNodeId: null,
      streamingNodeIds: new Set<string>(),
      previewNodeId: null,
    });
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
        const header = node.role === 'user' ? '## User' : '## Assistant';
        return `${header}\n\n${node.content}`;
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
  if (state.projectPath && state.isDirty) {
    try {
      await state.saveProject();
    } catch (error) {
      logger.error('Auto-save failed:', error);
    }
  }
}, 2000);

useGraphStore.subscribe((state, prevState) => {
  if (state.graph !== prevState.graph) {
    debouncedSave();
  }
});
