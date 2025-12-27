import { create } from 'zustand';
import { Node, Edge, applyNodeChanges, applyEdgeChanges, addEdge, NodeChange, EdgeChange, Connection } from '@xyflow/react';
import { invoke } from '@tauri-apps/api/core';
import {
  MessageNodeData,
  UserNodeData,
  AgentNodeData,
  UserFlowNodeData,
  AgentFlowNodeData,
  ThoughtTreeFlowNodeData,
  PermissionRequest,
  AgentProvider,
  ProviderStatus,
  ModelInfo,
  ModelPreferences,
  DEFAULT_PROVIDER,
} from '../types';
import { computeAutoLayout, type AutoLayoutOptions } from '../lib/graphLayout';

type ThoughtTreeNode = Node<ThoughtTreeFlowNodeData>;

// Project file format
interface ProjectFile {
  version: number;
  nodes: ThoughtTreeNode[];
  edges: Edge[];
  nodeData: Record<string, MessageNodeData>;
}

interface GraphState {
  // Graph data
  nodes: ThoughtTreeNode[];
  edges: Edge[];
  nodeData: Map<string, MessageNodeData>;

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
  streamingNodeId: string | null;
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
  setStreaming: (nodeId: string | null) => void;
  setEditing: (nodeId: string | null) => void;
  setPreviewNode: (nodeId: string | null) => void;
  togglePreviewNode: (nodeId: string) => void;
  deleteNode: (nodeId: string) => void;
  triggerSidePanelEditMode: () => void;
  clearSidePanelEditTrigger: () => void;

  // Context building
  buildConversationContext: (nodeId: string) => Array<{ role: string; content: string }>;

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

// Debounce helper
function debounce<T extends (...args: unknown[]) => unknown>(fn: T, delay: number): T {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  return ((...args: unknown[]) => {
    if (timeoutId) clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  }) as T;
}

export const useGraphStore = create<GraphState>()(
  (set, get) => ({
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
  streamingNodeId: null,
  editingNodeId: null,
  previewNodeId: null,
  pendingPermission: null,
  triggerSidePanelEdit: false,

  onNodesChange: (changes) => {
    set({ nodes: applyNodeChanges(changes, get().nodes) as ThoughtTreeNode[], isDirty: true });
  },

  onEdgesChange: (changes) => {
    set({ edges: applyEdgeChanges(changes, get().edges), isDirty: true });
  },

  onConnect: (connection) => {
    set({ edges: addEdge(connection, get().edges), isDirty: true });
  },

  selectNode: (id) => {
    set({ selectedNodeId: id });
  },

  createUserNode: (position = { x: 100, y: 100 }) => {
    const id = generateId();
    const data: UserNodeData = {
      id,
      role: 'user',
      content: '',
      timestamp: Date.now(),
    };

    const flowNodeData: UserFlowNodeData = {
      nodeData: data,
      isSelected: false,
    };

    const node: ThoughtTreeNode = {
      id,
      type: 'user',
      position,
      data: flowNodeData,
      dragHandle: '.thought-node',
    };

    set((state) => {
      const nodeData = new Map(state.nodeData);
      nodeData.set(id, data);
      return {
        nodes: [...state.nodes, node],
        nodeData,
        selectedNodeId: id,
        editingNodeId: id,
      };
    });

    return id;
  },

  createAgentNodeDownstream: (parentId, provider, model) => {
    const id = generateId();
    const activeProvider = provider ?? get().defaultProvider;
    const activeModel = model ?? get().getEffectiveModel(activeProvider);
    const data: AgentNodeData = {
      id,
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      provider: activeProvider,
      model: activeModel,
    };

    const COLLAPSED_NODE_HEIGHT = 120;
    const parentNode = get().nodes.find((n) => n.id === parentId);
    const position = parentNode
      ? { x: parentNode.position.x, y: parentNode.position.y + COLLAPSED_NODE_HEIGHT }
      : { x: 100, y: 100 };

    const flowNodeData: AgentFlowNodeData = {
      nodeData: data,
      isSelected: false,
    };

    const node: ThoughtTreeNode = {
      id,
      type: 'agent',
      position,
      data: flowNodeData,
      dragHandle: '.thought-node',
    };

    const edge: Edge = {
      id: `${parentId}-${id}`,
      source: parentId,
      target: id,
    };

    set((state) => {
      const nodeData = new Map(state.nodeData);
      nodeData.set(id, data);
      return {
        nodes: [...state.nodes, node],
        edges: [...state.edges, edge],
        nodeData,
        selectedNodeId: id,
        streamingNodeId: id,
      };
    });

    return id;
  },

  createUserNodeDownstream: (parentId) => {
    const id = generateId();
    const data: UserNodeData = {
      id,
      role: 'user',
      content: '',
      timestamp: Date.now(),
    };

    const COLLAPSED_NODE_HEIGHT = 120;
    const parentNode = get().nodes.find((n) => n.id === parentId);
    const position = parentNode
      ? { x: parentNode.position.x, y: parentNode.position.y + COLLAPSED_NODE_HEIGHT }
      : { x: 100, y: 100 };

    const flowNodeData: UserFlowNodeData = {
      nodeData: data,
      isSelected: false,
    };

    const node: ThoughtTreeNode = {
      id,
      type: 'user',
      position,
      data: flowNodeData,
      dragHandle: '.thought-node',
    };

    const edge: Edge = {
      id: `${parentId}-${id}`,
      source: parentId,
      target: id,
    };

    set((state) => {
      const nodeData = new Map(state.nodeData);
      nodeData.set(id, data);
      return {
        nodes: [...state.nodes, node],
        edges: [...state.edges, edge],
        nodeData,
        selectedNodeId: id,
        editingNodeId: id,
      };
    });

    return id;
  },

  updateNodeContent: (nodeId, content) => {
    set((state) => {
      const nodeData = new Map(state.nodeData);
      const data = nodeData.get(nodeId);
      if (!data) return state;

      const updated = { ...data, content } as MessageNodeData;
      nodeData.set(nodeId, updated);

      // Update the flow node data too
      const nodes = state.nodes.map((node) => {
        if (node.id !== nodeId) return node;
        return {
          ...node,
          data: {
            ...node.data,
            nodeData: updated,
          } as ThoughtTreeFlowNodeData,
        };
      }) as ThoughtTreeNode[];

      return { nodes, nodeData };
    });
  },

  appendToNode: (nodeId, chunk) => {
    set((state) => {
      const nodeData = new Map(state.nodeData);
      const data = nodeData.get(nodeId);
      if (!data) return state;

      const updated = { ...data, content: data.content + chunk } as MessageNodeData;
      nodeData.set(nodeId, updated);

      // Update the flow node data too
      const nodes = state.nodes.map((node) => {
        if (node.id !== nodeId) return node;
        return {
          ...node,
          data: {
            ...node.data,
            nodeData: updated,
          } as ThoughtTreeFlowNodeData,
        };
      }) as ThoughtTreeNode[];

      return { nodes, nodeData };
    });
  },

  setStreaming: (nodeId) => {
    console.log('[Store] setStreaming called with:', nodeId);
    // Streaming state is now derived from streamingNodeId, so just update that
    set({ streamingNodeId: nodeId });
    console.log('[Store] streamingNodeId after set:', get().streamingNodeId);
  },

  setEditing: (nodeId) => {
    set({ editingNodeId: nodeId });
  },

  setPreviewNode: (nodeId) => {
    set({ previewNodeId: nodeId });
  },

  togglePreviewNode: (nodeId) => {
    set((state) => ({
      previewNodeId: state.previewNodeId === nodeId ? null : nodeId,
    }));
  },

  deleteNode: (nodeId) => {
    set((state) => {
      const nodeData = new Map(state.nodeData);
      nodeData.delete(nodeId);

      const nodes = state.nodes.filter((n) => n.id !== nodeId);
      const edges = state.edges.filter((e) => e.source !== nodeId && e.target !== nodeId);

      return {
        nodes,
        edges,
        nodeData,
        selectedNodeId: state.selectedNodeId === nodeId ? null : state.selectedNodeId,
        editingNodeId: state.editingNodeId === nodeId ? null : state.editingNodeId,
        previewNodeId: state.previewNodeId === nodeId ? null : state.previewNodeId,
      };
    });
  },

  triggerSidePanelEditMode: () => {
    set({ triggerSidePanelEdit: true });
  },

  clearSidePanelEditTrigger: () => {
    set({ triggerSidePanelEdit: false });
  },

  buildConversationContext: (nodeId) => {
    const { edges, nodeData } = get();

    // Build parent lookup (target -> source)
    const parentMap = new Map<string, string>();
    edges.forEach((edge) => {
      parentMap.set(edge.target, edge.source);
    });

    // Traverse up the parent chain
    const messages: Array<{ role: string; content: string }> = [];
    const visited = new Set<string>();
    let currentId: string | undefined = nodeId;

    while (currentId && !visited.has(currentId)) {
      visited.add(currentId);
      const data = nodeData.get(currentId);
      if (data && data.content.trim()) {
        messages.unshift({ role: data.role, content: data.content });
      }
      currentId = parentMap.get(currentId);
    }

    return messages;
  },

  setSummary: (nodeId, summary) => {
    set((state) => {
      const nodeData = new Map(state.nodeData);
      const data = nodeData.get(nodeId);
      if (!data) return state;

      const updated = {
        ...data,
        summary,
        summaryTimestamp: Date.now(),
      } as MessageNodeData;
      nodeData.set(nodeId, updated);

      // Update the flow node data too
      const nodes = state.nodes.map((node) => {
        if (node.id !== nodeId) return node;
        return {
          ...node,
          data: {
            ...node.data,
            nodeData: updated,
          } as ThoughtTreeFlowNodeData,
        };
      }) as ThoughtTreeNode[];

      return { nodes, nodeData };
    });
  },

  setPendingPermission: (permission) => {
    set({ pendingPermission: permission });
  },

  // Provider actions
  setDefaultProvider: (provider) => {
    set({ defaultProvider: provider });
  },

  setAvailableProviders: (providers) => {
    set({ availableProviders: providers });
  },

  // Model actions
  setGlobalModelPreferences: (preferences) => {
    set({ globalModelPreferences: preferences });
  },

  setGlobalModelPreference: (provider, modelId) => {
    const current = get().globalModelPreferences;
    set({
      globalModelPreferences: {
        ...current,
        [provider]: modelId ?? undefined,
      },
    });
  },

  setProjectModelPreferences: (preferences) => {
    set({ projectModelPreferences: preferences, isDirty: preferences !== null });
  },

  setProjectModelPreference: (provider, modelId) => {
    const current = get().projectModelPreferences ?? {};
    set({
      projectModelPreferences: {
        ...current,
        [provider]: modelId ?? undefined,
      },
      isDirty: true,
    });
  },

  setAvailableModels: (provider, models) => {
    const current = get().availableModels;
    set({
      availableModels: {
        ...current,
        [provider]: models,
      },
    });
  },

  getEffectiveModel: (provider) => {
    const { projectModelPreferences, globalModelPreferences } = get();
    // Project-level overrides global
    const projectModel = projectModelPreferences?.[provider];
    if (projectModel) return projectModel;
    // Fall back to global
    return globalModelPreferences[provider];
  },

  // Project actions
  setProjectPath: (path) => {
    set({ projectPath: path });
  },

  saveProject: async () => {
    const { projectPath, nodes, edges, nodeData } = get();
    if (!projectPath) {
      console.warn('No project path set, cannot save');
      return;
    }

    const projectFile: ProjectFile = {
      version: 1,
      nodes,
      edges,
      nodeData: Object.fromEntries(nodeData),
    };

    try {
      await invoke('save_project', {
        path: projectPath,
        data: JSON.stringify(projectFile, null, 2),
      });
      set({ lastSavedAt: Date.now(), isDirty: false });
      console.log('Project saved to:', projectPath);
    } catch (error) {
      console.error('Failed to save project:', error);
      throw error;
    }
  },

  loadProject: async (path) => {
    try {
      const data = await invoke<string>('load_project', { path });
      const projectFile: ProjectFile = JSON.parse(data);

      // Migrate nodeData: agent nodes without provider default to 'claude-code'
      const migratedNodeData: Record<string, MessageNodeData> = {};
      for (const [id, node] of Object.entries(projectFile.nodeData)) {
        if (node.role === 'assistant' && !('provider' in node)) {
          migratedNodeData[id] = {
            ...node,
            provider: DEFAULT_PROVIDER,
          } as AgentNodeData;
        } else {
          migratedNodeData[id] = node;
        }
      }

      // Convert nodeData from object back to Map
      const nodeDataMap = new Map(Object.entries(migratedNodeData));

      // Migrate nodes to include dragHandle if missing (for existing saved projects)
      const migratedNodes = projectFile.nodes.map(node => ({
        ...node,
        dragHandle: node.dragHandle ?? '.thought-node',
      }));

      set({
        nodes: migratedNodes,
        edges: projectFile.edges,
        nodeData: nodeDataMap,
        projectPath: path,
        lastSavedAt: Date.now(),
        isDirty: false,
        selectedNodeId: null,
        editingNodeId: null,
        streamingNodeId: null,
        previewNodeId: null,
      });

      // Track in recently opened projects
      try {
        await invoke('add_recent_project', { path });
      } catch (error) {
        console.warn('Failed to update recent projects:', error);
      }

      console.log('Project loaded from:', path);
    } catch (error) {
      console.error('Failed to load project:', error);
      throw error;
    }
  },

  newProject: () => {
    set({
      nodes: [],
      edges: [],
      nodeData: new Map(),
      projectPath: null,
      lastSavedAt: null,
      isDirty: false,
      selectedNodeId: null,
      editingNodeId: null,
      streamingNodeId: null,
      previewNodeId: null,
    });
  },

  exportSubgraph: (nodeIds) => {
    const { nodeData, edges } = get();

    // Build an ordered list from the node IDs following edges
    // For a linear path, we need to order them correctly
    const nodeSet = new Set(nodeIds);

    // Find starting node (has no incoming edge from selected nodes)
    const hasIncoming = new Set<string>();
    edges.forEach((edge) => {
      if (nodeSet.has(edge.source) && nodeSet.has(edge.target)) {
        hasIncoming.add(edge.target);
      }
    });

    let startNode = nodeIds.find((id) => !hasIncoming.has(id)) || nodeIds[0];

    // Build ordered list by following edges
    const ordered: string[] = [];
    const visited = new Set<string>();
    let current: string | undefined = startNode;

    while (current && !visited.has(current) && nodeSet.has(current)) {
      visited.add(current);
      ordered.push(current);
      // Find next node
      const nextEdge = edges.find(
        (e) => e.source === current && nodeSet.has(e.target)
      );
      current = nextEdge?.target;
    }

    // Generate markdown
    return ordered
      .map((id) => {
        const data = nodeData.get(id);
        if (!data) return '';
        const roleHeader = data.role === 'user' ? '## User' : '## Assistant';
        return `${roleHeader}\n\n${data.content}`;
      })
      .filter(Boolean)
      .join('\n\n---\n\n');
  },

  autoLayout: (options) => {
    const { nodes, edges } = get();
    if (nodes.length === 0) return;

    const pos = computeAutoLayout(nodes, edges, options);
    set({
      nodes: nodes.map((n) => {
        const p = pos.get(n.id);
        if (!p) return n;
        return { ...n, position: p };
      }) as ThoughtTreeNode[],
      isDirty: true,
    });
  },
}));

// Auto-save subscription
const debouncedSave = debounce(async () => {
  const state = useGraphStore.getState();
  if (state.projectPath && state.isDirty) {
    try {
      await state.saveProject();
    } catch (error) {
      console.error('Auto-save failed:', error);
    }
  }
}, 2000);

// Subscribe to changes that should trigger auto-save
useGraphStore.subscribe((state, prevState) => {
  if (
    state.nodes !== prevState.nodes ||
    state.edges !== prevState.edges ||
    state.nodeData !== prevState.nodeData
  ) {
    debouncedSave();
  }
});
