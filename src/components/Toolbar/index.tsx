import { useState } from 'react';
import { useGraphStore } from '../../store/useGraphStore';
import { useUIStore } from '../../store/useUIStore';
import { SettingsDialog } from '../SettingsDialog';
import { addRecentProject, exportMarkdown, newProjectDialog, openProjectDialog, pickKagiExport } from '../../lib/desktop';
import { getBackendTransport } from '../../lib/transport';
import { logger } from '../../lib/logger';
import './Toolbar.css';

export function Toolbar() {
  const projectPath = useGraphStore((state) => state.projectPath);
  const projectTitle = useGraphStore((state) => state.projectTitle);
  const isDirty = useGraphStore((state) => state.isDirty);
  const lastSavedAt = useGraphStore((state) => state.lastSavedAt);
  const nodes = useGraphStore((state) => state.nodes);
  const selectedNodeId = useGraphStore((state) => state.selectedNodeId);
  const setProjectPath = useGraphStore((state) => state.setProjectPath);
  const saveProject = useGraphStore((state) => state.saveProject);
  const loadProject = useGraphStore((state) => state.loadProject);
  const newProject = useGraphStore((state) => state.newProject);
  const importGraph = useGraphStore((state) => state.importGraph);
  const exportSubgraph = useGraphStore((state) => state.exportSubgraph);
  const nodeData = useGraphStore((state) => state.nodeData);
  const createUserNodeDownstream = useGraphStore((state) => state.createUserNodeDownstream);
  const isNodeBlocked = useGraphStore((state) => state.isNodeBlocked);
  const autoLayout = useGraphStore((state) => state.autoLayout);
  const getConversationPathNodeIds = useGraphStore((state) => state.getConversationPathNodeIds);
  const nativeDialogsEnabled = getBackendTransport().capabilities.nativeDialogs;

  const [isSaving, setIsSaving] = useState(false);
  // In the UI store (not local state) so the Palette can suppress ⌘K while open.
  const showSettings = useUIStore((state) => state.settingsOpen);
  const setShowSettings = useUIStore((state) => state.setSettingsOpen);

  // Check if selected node is an agent node (can reply)
  const selectedNodeData = selectedNodeId ? nodeData.get(selectedNodeId) : null;
  const canReply = selectedNodeData?.role === 'assistant' && selectedNodeId && !isNodeBlocked(selectedNodeId);

  const handleReply = () => {
    if (selectedNodeId && canReply) {
      createUserNodeDownstream(selectedNodeId);
    }
  };

  const handleCleanUp = () => {
    autoLayout({ direction: 'TB', gridSize: 20 });
  };

  // Get project name from path
  const projectName = projectPath
    ? projectPath.split('/').pop()?.replace('.thoughttree', '') || 'Untitled'
    : projectTitle || 'Untitled';

  const handleImport = async () => {
    if (!nativeDialogsEnabled) return;
    try {
      const path = await pickKagiExport();
      if (path) {
        const imported = await getBackendTransport().importKagiExport(path);
        importGraph(imported.title, imported.graph);
      }
    } catch (error) {
      logger.error('Failed to import Kagi export:', error);
      window.alert(error instanceof Error ? error.message : String(error));
    }
  };

  const handleNewProject = async () => {
    if (!nativeDialogsEnabled) return;
    try {
      const path = await newProjectDialog();
      if (path) {
        newProject();
        setProjectPath(path);
        await saveProject();
        try {
          await addRecentProject(path);
        } catch (error) {
          logger.warn('Failed to track new project:', error);
        }
      }
    } catch (error) {
      logger.error('Failed to create new project:', error);
    }
  };

  const handleOpenProject = async () => {
    if (!nativeDialogsEnabled) return;
    try {
      const path = await openProjectDialog();
      if (path) {
        await loadProject(path);
        try {
          await addRecentProject(path);
        } catch (error) {
          logger.warn('Failed to track project:', error);
        }
      }
    } catch (error) {
      logger.error('Failed to open project:', error);
    }
  };

  const handleSaveProject = async () => {
    if (!projectPath) {
      // No project yet, show save dialog
      await handleSaveProjectAs();
      return;
    }

    setIsSaving(true);
    try {
      await saveProject();
    } catch (error) {
      logger.error('Failed to save project:', error);
    } finally {
      setIsSaving(false);
    }
  };

  const handleSaveProjectAs = async () => {
    if (!nativeDialogsEnabled) return;
    try {
      const path = await newProjectDialog();
      if (path) {
        setProjectPath(path);
        await saveProject();
      }
    } catch (error) {
      logger.error('Failed to save project:', error);
    }
  };

  const handleExportSelected = async () => {
    if (!selectedNodeId) return;

    // Export exact lineage IDs to avoid content-based collisions.
    const nodeIds = getConversationPathNodeIds(selectedNodeId);

    if (nodeIds.length === 0) {
      // Just export the selected node
      const markdown = exportSubgraph([selectedNodeId]);
      await doExport(markdown, 'export.md');
    } else {
      const markdown = exportSubgraph(nodeIds);
      await doExport(markdown, 'conversation-export.md');
    }
  };

  const handleExportAll = async () => {
    if (nodes.length === 0) return;

    const allNodeIds = nodes.map((n) => n.id);
    const markdown = exportSubgraph(allNodeIds);
    await doExport(markdown, 'full-export.md');
  };

  const doExport = async (content: string, defaultName: string) => {
    if (!nativeDialogsEnabled) return;
    try {
      const path = await exportMarkdown(content, defaultName);
      if (path) {
        logger.info('Exported to:', path);
      }
    } catch (error) {
      logger.error('Failed to export:', error);
    }
  };

  // Format last saved time
  const formatLastSaved = () => {
    if (!lastSavedAt) return null;
    const seconds = Math.floor((Date.now() - lastSavedAt) / 1000);
    if (seconds < 60) return 'Saved just now';
    if (seconds < 3600) return `Saved ${Math.floor(seconds / 60)}m ago`;
    return `Saved ${Math.floor(seconds / 3600)}h ago`;
  };

  return (
    <div className="toolbar">
      <div className="toolbar-left">
        <span className="project-name">
          {projectName}
          {isDirty && <span className="unsaved-indicator">*</span>}
        </span>
        {lastSavedAt && (
          <span className="last-saved">{formatLastSaved()}</span>
        )}
      </div>

      <div className="toolbar-center">
        <button onClick={handleNewProject} title="New Project" disabled={!nativeDialogsEnabled}>
          New
        </button>
        <button onClick={handleOpenProject} title="Open Project" disabled={!nativeDialogsEnabled}>
          Open
        </button>
        <button onClick={handleImport} title="Import Kagi export" disabled={!nativeDialogsEnabled}>
          Import
        </button>
        <button
          onClick={handleSaveProject}
          disabled={isSaving || (!isDirty && projectPath !== null)}
          title="Save Project"
        >
          {isSaving ? 'Saving...' : 'Save'}
        </button>
        <span className="toolbar-divider" />
        <button
          onClick={handleCleanUp}
          disabled={nodes.length === 0}
          title="Tidy graph (Cmd/Ctrl+L)"
        >
          Tidy graph
        </button>
        <span className="toolbar-divider" />
        <button
          onClick={handleReply}
          disabled={!canReply}
          title="Reply to selected agent node (Enter)"
        >
          Reply
        </button>
        <span className="toolbar-divider" />
        <button
          onClick={handleExportSelected}
          disabled={!selectedNodeId || !nativeDialogsEnabled}
          title="Export conversation to selected node"
        >
          Export Thread
        </button>
        <button
          onClick={handleExportAll}
          disabled={nodes.length === 0 || !nativeDialogsEnabled}
          title="Export all nodes"
        >
          Export All
        </button>
      </div>

      <div className="toolbar-right">
        <button
          onClick={() => setShowSettings(true)}
          title="Settings"
          className="settings-button"
        >
          Settings
        </button>
        <span className="node-count">{nodes.length} nodes</span>
      </div>

      <SettingsDialog
        isOpen={showSettings}
        onClose={() => setShowSettings(false)}
      />
    </div>
  );
}
