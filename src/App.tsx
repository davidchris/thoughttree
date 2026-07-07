import { useEffect, useState, useCallback } from 'react';
import { ReactFlowProvider } from '@xyflow/react';
import { Graph } from './components/Graph';
import { Toolbar } from './components/Toolbar';
import { PermissionDialog } from './components/PermissionDialog';
import { SetupWizard } from './components/SetupWizard';
import { ProjectOpeningWizard } from './components/ProjectOpeningWizard';
import { SidePanel } from './components/SidePanel';
import { Palette } from './components/Palette';
import { getNotesDirectory, newProjectDialog, openProjectDialog, addRecentProject } from './lib/desktop';
import { getBackendTransport } from './lib/transport';
import { useSummaryGeneration } from './hooks/useSummaryGeneration';
import { useGraphStore } from './store/useGraphStore';
import { useProviderStore } from './store/useProviderStore';
import { useUIStore } from './store/useUIStore';
import { logger } from './lib/logger';
import './App.css';

function App() {
  const [isLoading, setIsLoading] = useState(true);
  const [needsSetup, setNeedsSetup] = useState(false);
  const transport = getBackendTransport();
  const projectPath = useGraphStore((state) => state.projectPath);
  const loadProject = useGraphStore((state) => state.loadProject);
  const newProject = useGraphStore((state) => state.newProject);
  const setProjectPath = useGraphStore((state) => state.setProjectPath);
  const saveProject = useGraphStore((state) => state.saveProject);

  // Automatically generate summaries for node content
  useSummaryGeneration();

  useEffect(() => {
    const unsubscribeStream = transport.onStreamChunk(({ nodeId, chunk }) => {
      useGraphStore.getState().appendToNode(nodeId, chunk);
    });
    const unsubscribePermission = transport.onPermissionRequest((permission) => {
      useUIStore.getState().setPendingPermission(permission);
    });

    const initialize = async () => {
      try {
        if (transport.capabilities.nativeDialogs) {
          const notesDir = await getNotesDirectory();
          setNeedsSetup(!notesDir);
        } else {
          setNeedsSetup(false);
        }

        // Load provider configuration
        try {
          const providers = await transport.getAvailableProviders();
          const defaultProv = await transport.getDefaultProvider();
          useProviderStore.getState().setAvailableProviders(providers);
          useProviderStore.getState().setDefaultProvider(defaultProv);
        } catch (error) {
          logger.warn('Failed to load provider config:', error);
        }
      } catch (error) {
        logger.error('Failed to initialize:', error);
      } finally {
        setIsLoading(false);
      }
    };

    initialize();
    return () => {
      unsubscribeStream();
      unsubscribePermission();
    };
  }, [transport]);

  const handleOpenProject = useCallback(async () => {
    if (!transport.capabilities.nativeDialogs) return;
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
  }, [loadProject, transport.capabilities.nativeDialogs]);

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
        logger.error('Failed to load project:', error);
      }
    },
    [loadProject]
  );

  const handleOpenDialog = useCallback(async () => {
    if (!transport.capabilities.nativeDialogs) return;
    try {
      const path = await openProjectDialog();
      if (path) {
        await handleProjectSelected(path);
        try {
          await addRecentProject(path);
        } catch (error) {
          logger.warn('Failed to track project:', error);
        }
      }
    } catch (error) {
      logger.error('Failed to open project:', error);
    }
  }, [handleProjectSelected, transport.capabilities.nativeDialogs]);

  const handleNewProject = useCallback(async () => {
    if (!transport.capabilities.nativeDialogs) return;
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
  }, [newProject, saveProject, setProjectPath, transport.capabilities.nativeDialogs]);

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
        nativeDialogsEnabled={transport.capabilities.nativeDialogs}
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
        <Palette />
      </ReactFlowProvider>
      <PermissionDialog />
    </div>
  );
}

export default App;
