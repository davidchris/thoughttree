import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useGraphStore } from '../../store/useGraphStore';
import { SettingsDialog } from '../SettingsDialog';
import './Toolbar.css';

export function Toolbar() {
  const {
    projectPath,
    isDirty,
    lastSavedAt,
    nodes,
    selectedNodeId,
    setProjectPath,
    saveProject,
    loadProject,
    newProject,
    exportSubgraph,
    buildConversationContext,
    nodeData,
    createUserNodeDownstream,
    streamingNodeId,
    autoLayout,
  } = useGraphStore();

  const [isSaving, setIsSaving] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  // Check if selected node is an agent node (can reply)
  const selectedNodeData = selectedNodeId ? nodeData.get(selectedNodeId) : null;
  const canReply = selectedNodeData?.role === 'assistant' && !streamingNodeId;

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
          console.warn('Failed to track new project:', error);
        }
      }
    } catch (error) {
      console.error('Failed to create new project:', error);
    }
  };

  const handleOpenProject = async () => {
    try {
      const path = await invoke<string | null>('open_project_dialog');
      if (path) {
        await loadProject(path);
      }
    } catch (error) {
      console.error('Failed to open project:', error);
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
      console.error('Failed to save project:', error);
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
      console.error('Failed to save project:', error);
    }
  };

  const handleExportSelected = async () => {
    if (!selectedNodeId) return;

    // Export the conversation path leading to the selected node
    const context = buildConversationContext(selectedNodeId);
    const nodeIds = nodes
      .filter((n) => context.some((c) => {
        const data = useGraphStore.getState().nodeData.get(n.id);
        return data?.content === c.content && data?.role === c.role;
      }))
      .map((n) => n.id);

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
        console.log('Exported to:', path);
      }
    } catch (error) {
      console.error('Failed to export:', error);
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
