import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useGraphStore } from '../../store/useGraphStore';
import { SettingsDialog } from '../SettingsDialog';
import { logger } from '../../lib/logger';
import './Toolbar.css';

export function Toolbar() {
  const projectPath = useGraphStore((state) => state.projectPath);
  const isDirty = useGraphStore((state) => state.isDirty);
  const lastSavedAt = useGraphStore((state) => state.lastSavedAt);
  const nodes = useGraphStore((state) => state.nodes);
  const selectedNodeId = useGraphStore((state) => state.selectedNodeId);
  const setProjectPath = useGraphStore((state) => state.setProjectPath);
  const saveProject = useGraphStore((state) => state.saveProject);
  const loadProject = useGraphStore((state) => state.loadProject);
  const newProject = useGraphStore((state) => state.newProject);
  const exportSubgraph = useGraphStore((state) => state.exportSubgraph);
  const nodeData = useGraphStore((state) => state.nodeData);
  const createUserNodeDownstream = useGraphStore((state) => state.createUserNodeDownstream);
  const isNodeBlocked = useGraphStore((state) => state.isNodeBlocked);
  const autoLayout = useGraphStore((state) => state.autoLayout);
  const getConversationPathNodeIds = useGraphStore((state) => state.getConversationPathNodeIds);

  const [isSaving, setIsSaving] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

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
    : 'Untitled';

  const handleNewProject = async () => {
    try {
      const path = await invoke<string | null>('new_project_dialog');
      if (path) {
        newProject();
        setProjectPath(path);
        await saveProject();
        // Track new project in recent projects
        try {
          await invoke('add_recent_project', { path });
        } catch (error) {
          logger.warn('Failed to track new project:', error);
        }
      }
    } catch (error) {
      logger.error('Failed to create new project:', error);
    }
  };

  const handleOpenProject = async () => {
    try {
      const path = await invoke<string | null>('open_project_dialog');
      if (path) {
        await loadProject(path);
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
    try {
      const path = await invoke<string | null>('new_project_dialog');
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
    try {
      const path = await invoke<string | null>('export_markdown', {
        content,
        defaultName,
      });
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
        <button onClick={handleNewProject} title="New Project">
          New
        </button>
        <button onClick={handleOpenProject} title="Open Project">
          Open
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
          disabled={!selectedNodeId}
          title="Export conversation to selected node"
        >
          Export Thread
        </button>
        <button
          onClick={handleExportAll}
          disabled={nodes.length === 0}
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
