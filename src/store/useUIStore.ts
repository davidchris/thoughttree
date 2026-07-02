import { create } from 'zustand';
import { PermissionRequest } from '../types';

/** Keep in sync with the palette-flash animation duration in
 * src/components/Graph/styles.css. */
const FLASH_DURATION_MS = 1000;

let flashTimer: ReturnType<typeof setTimeout> | undefined;

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
  /** Node briefly highlighted after a Palette jump. */
  flashNodeId: string | null;
  pendingPermission: PermissionRequest | null;
  settingsOpen: boolean;
  triggerSidePanelEdit: boolean;

  setEditing: (nodeId: string | null) => void;
  setPreviewNode: (nodeId: string | null) => void;
  togglePreviewNode: (nodeId: string) => void;
  /** Flash a node briefly; owns its own expiry so callers can't leak the state. */
  flashNode: (nodeId: string) => void;
  setPendingPermission: (permission: PermissionRequest | null) => void;
  setSettingsOpen: (open: boolean) => void;
  triggerSidePanelEditMode: () => void;
  clearSidePanelEditTrigger: () => void;

  /** Drop references to a node that no longer exists. */
  clearNodeRefs: (nodeId: string) => void;
  /** Reset everything, e.g. when a project is loaded or closed. */
  reset: () => void;
}

export const useUIStore = create<UIState>()((set, get) => ({
  editingNodeId: null,
  previewNodeId: null,
  flashNodeId: null,
  pendingPermission: null,
  settingsOpen: false,
  triggerSidePanelEdit: false,

  setEditing: (nodeId) => set({ editingNodeId: nodeId }),
  setPreviewNode: (nodeId) => set({ previewNodeId: nodeId }),
  togglePreviewNode: (nodeId) =>
    set((state) => ({ previewNodeId: state.previewNodeId === nodeId ? null : nodeId })),
  flashNode: (nodeId) => {
    clearTimeout(flashTimer);
    const arm = () => {
      set({ flashNodeId: nodeId });
      flashTimer = setTimeout(() => set({ flashNodeId: null }), FLASH_DURATION_MS);
    };
    if (get().flashNodeId === nodeId) {
      // Re-flashing the same node: the class must toggle off for one frame or
      // the CSS animation never restarts.
      set({ flashNodeId: null });
      requestAnimationFrame(arm);
    } else {
      arm();
    }
  },
  setPendingPermission: (permission) => set({ pendingPermission: permission }),
  setSettingsOpen: (open) => set({ settingsOpen: open }),
  triggerSidePanelEditMode: () => set({ triggerSidePanelEdit: true }),
  clearSidePanelEditTrigger: () => set({ triggerSidePanelEdit: false }),

  clearNodeRefs: (nodeId) =>
    set((state) => ({
      editingNodeId: state.editingNodeId === nodeId ? null : state.editingNodeId,
      previewNodeId: state.previewNodeId === nodeId ? null : state.previewNodeId,
      flashNodeId: state.flashNodeId === nodeId ? null : state.flashNodeId,
    })),

  // settingsOpen deliberately survives reset(): the Settings dialog is
  // app-level, and loadProject/newProject call reset() — closing it there
  // would discard in-progress edits on every project switch.
  reset: () => {
    clearTimeout(flashTimer);
    set({
      editingNodeId: null,
      previewNodeId: null,
      flashNodeId: null,
      pendingPermission: null,
      triggerSidePanelEdit: false,
    });
  },
}));
