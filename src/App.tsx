import { useEffect, useState, useCallback } from 'react';
import { ReactFlowProvider } from '@xyflow/react';
import { invoke } from '@tauri-apps/api/core';
import { Graph } from './components/Graph';
import { Toolbar } from './components/Toolbar';
import { PermissionDialog } from './components/PermissionDialog';
import { SetupWizard } from './components/SetupWizard';
import { SidePanel } from './components/SidePanel';
import { initializeListeners } from './lib/tauri';
import { useSummaryGeneration } from './hooks/useSummaryGeneration';
import { useGraphStore } from './store/useGraphStore';
import './App.css';

function App() {
  const [isLoading, setIsLoading] = useState(true);
  const [needsSetup, setNeedsSetup] = useState(false);
  const loadProject = useGraphStore((state) => state.loadProject);

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
