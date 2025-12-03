import { useEffect, useState } from 'react';
import { ReactFlowProvider } from '@xyflow/react';
import { invoke } from '@tauri-apps/api/core';
import { Graph } from './components/Graph';
import { Toolbar } from './components/Toolbar';
import { PermissionDialog } from './components/PermissionDialog';
import { SetupWizard } from './components/SetupWizard';
import { initializeListeners } from './lib/tauri';
import './App.css';

function App() {
  const [isLoading, setIsLoading] = useState(true);
  const [needsSetup, setNeedsSetup] = useState(false);

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
        <Graph />
      </ReactFlowProvider>
      <PermissionDialog />
    </div>
  );
}

export default App;
