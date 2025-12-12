import { useEffect, useState, useCallback } from 'react';
import { ReactFlowProvider } from '@xyflow/react';
import { invoke } from '@tauri-apps/api/core';
import { Graph } from './components/Graph';
import { Toolbar } from './components/Toolbar';
import { PermissionDialog } from './components/PermissionDialog';
import { SetupWizard } from './components/SetupWizard';
import { ProjectOpeningWizard } from './components/ProjectOpeningWizard';
import { SidePanel } from './components/SidePanel';
import { initializeListeners } from './lib/tauri';
import { useSummaryGeneration } from './hooks/useSummaryGeneration';
import { useGraphStore } from './store/useGraphStore';
import './App.css';

function App() {
  const [isLoading, setIsLoading] = useState(true);
  const [needsSetup, setNeedsSetup] = useState(false);
  const projectPath = useGraphStore((state) => state.projectPath);
  const loadProject = useGraphStore((state) => state.loadProject);
  const newProject = useGraphStore((state) => state.newProject);
  const setProjectPath = useGraphStore((state) => state.setProjectPath);
  const saveProject = useGraphStore((state) => state.saveProject);

  // Automatically generate summaries for node content
  useSummaryGeneration();

  useEffect(() => {
    const initialize = async () => {
      try {
        // Check if notes directory is configured
        const notesDir = await invoke<string | null>('get_notes_directory');
        setNeedsSetup(!notesDir);

        // Initialize event listeners
        await initializeListeners();
      } catch (error) {
        console.error('Failed to initialize:', error);
      } finally {
        setIsLoading(false);
      }
    };

    initialize();
  }, []);

  const handleOpenProject = useCallback(async () => {
    try {
      const path = await invoke<string | null>('open_project_dialog');
      if (path) {
        await loadProject(path);
      }
    } catch (error) {
      console.error('Failed to open project:', error);
    }
  }, [loadProject]);

  // Global keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Cmd/Ctrl + O to open project
      if ((e.metaKey || e.ctrlKey) && e.key === 'o') {
        e.preventDefault();
        const target = e.target as HTMLElement;
        // Don't trigger if typing in an input or textarea
        if (target.tagName === 'TEXTAREA' || target.tagName === 'INPUT' || target.isContentEditable) {
          return;
        }
        handleOpenProject();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleOpenProject]);

  const handleSetupComplete = () => {
    setNeedsSetup(false);
  };

  const handleProjectSelected = useCallback(
    async (path: string) => {
      try {
        await loadProject(path);
        // Track in recent projects (handled by ProjectOpeningWizard)
      } catch (error) {
        console.error('Failed to load project:', error);
      }
    },
    [loadProject]
  );

  const handleOpenDialog = useCallback(async () => {
    try {
      const path = await invoke<string | null>('open_project_dialog');
      if (path) {
        await handleProjectSelected(path);
      }
    } catch (error) {
      console.error('Failed to open project:', error);
    }
  }, [handleProjectSelected]);

  const handleNewProject = useCallback(async () => {
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
  }, [newProject, setProjectPath, saveProject]);

  if (isLoading) {
    return (
      <div className="app loading">
        <p>Loading...</p>
      </div>
    );
  }

  if (needsSetup) {
    return <SetupWizard onComplete={handleSetupComplete} />;
  }

  // Show project opening wizard if no project is loaded
  if (!projectPath) {
    return (
      <ProjectOpeningWizard
        onProjectSelected={handleProjectSelected}
        onOpenDialog={handleOpenDialog}
        onNewProject={handleNewProject}
      />
    );
  }

  return (
    <div className="app">
      <ReactFlowProvider>
        <Toolbar />
        <div className="app-main">
          <Graph />
          <SidePanel />
        </div>
      </ReactFlowProvider>
      <PermissionDialog />
    </div>
  );
}

export default App;
