import { create } from 'zustand';
import { PermissionRequest } from '../types';

/**
 * Transient UI state that is never persisted to the project file.
 *
 * Node selection (`selectedNodeId`) and streaming state intentionally live in
 * useGraphStore: both feed the graph projection, so splitting them out would
 * force every projection to read across stores.
 */
interface UIState {
  editingNodeId: string | null;
  previewNodeId: string | null;
  pendingPermission: PermissionRequest | null;
  triggerSidePanelEdit: boolean;

  setEditing: (nodeId: string | null) => void;
  setPreviewNode: (nodeId: string | null) => void;
  togglePreviewNode: (nodeId: string) => void;
  setPendingPermission: (permission: PermissionRequest | null) => void;
  triggerSidePanelEditMode: () => void;
  clearSidePanelEditTrigger: () => void;

  /** Drop references to a node that no longer exists. */
  clearNodeRefs: (nodeId: string) => void;
  /** Reset everything, e.g. when a project is loaded or closed. */
  reset: () => void;
}

export const useUIStore = create<UIState>()((set) => ({
  editingNodeId: null,
  previewNodeId: null,
  pendingPermission: null,
  triggerSidePanelEdit: false,

  setEditing: (nodeId) => set({ editingNodeId: nodeId }),
  setPreviewNode: (nodeId) => set({ previewNodeId: nodeId }),
  togglePreviewNode: (nodeId) =>
    set((state) => ({ previewNodeId: state.previewNodeId === nodeId ? null : nodeId })),
  setPendingPermission: (permission) => set({ pendingPermission: permission }),
  triggerSidePanelEditMode: () => set({ triggerSidePanelEdit: true }),
  clearSidePanelEditTrigger: () => set({ triggerSidePanelEdit: false }),

  clearNodeRefs: (nodeId) =>
    set((state) => ({
      editingNodeId: state.editingNodeId === nodeId ? null : state.editingNodeId,
      previewNodeId: state.previewNodeId === nodeId ? null : state.previewNodeId,
    })),

  reset: () =>
    set({
      editingNodeId: null,
      previewNodeId: null,
      pendingPermission: null,
      triggerSidePanelEdit: false,
    }),
}));
