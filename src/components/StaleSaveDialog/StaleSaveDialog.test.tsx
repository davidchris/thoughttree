import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { GRAPH_JSON_VERSION, GraphMutations, GraphSerialize } from '@thoughttree/graph-model';
import type { BackendTransport } from '../../lib/transport';
import { setBackendTransport } from '../../lib/transport';
import { useGraphStore } from '../../store/useGraphStore';
import { useProviderStore } from '../../store/useProviderStore';
import { useUIStore } from '../../store/useUIStore';
import { StaleSaveDialog } from './index';

function resetStores() {
  useGraphStore.getState().newProject();
  useProviderStore.setState({
    globalModelPreferences: {},
    globalEffortPreferences: {},
  });
  useUIStore.getState().reset();
}

function createMockTransport(): BackendTransport {
  return {
    capabilities: { nativeDialogs: true },
    loadProject: vi.fn(),
    saveProject: vi.fn(),
    listProjects: vi.fn(),
    importKagiExport: vi.fn(),
    sendPrompt: vi.fn(),
    respondToPermission: vi.fn(),
    checkAcpAvailable: vi.fn(),
    searchFiles: vi.fn(),
    getAvailableProviders: vi.fn(),
    getDefaultProvider: vi.fn(),
    setDefaultProvider: vi.fn(),
    getModelPreferences: vi.fn(),
    setModelPreference: vi.fn(),
    getEffortPreferences: vi.fn(),
    setEffortPreference: vi.fn(),
    getAvailableModels: vi.fn(),
    generateSummary: vi.fn(),
    onStreamChunk: vi.fn(() => () => {}),
    onPermissionRequest: vi.fn(() => () => {}),
  };
}

describe('StaleSaveDialog', () => {
  let transport: BackendTransport;

  beforeEach(() => {
    transport = createMockTransport();
    setBackendTransport(transport);
    resetStores();
    useGraphStore.setState({
      projectPath: '/tmp/project.thoughttree',
      projectRevision: 'rev-1',
    });
  });

  it('reloads the current project and dismisses the dialog', async () => {
    vi.mocked(transport.loadProject).mockResolvedValue({
      data: JSON.stringify({
        version: GRAPH_JSON_VERSION,
        graph: GraphSerialize.toJSON(GraphMutations.empty()),
      }),
      revision: 'rev-2',
    });
    useUIStore.getState().setStaleProjectSave({
      path: '/tmp/project.thoughttree',
      currentRevision: 'rev-2',
    });

    render(<StaleSaveDialog />);
    await userEvent.click(screen.getByRole('button', { name: 'Reload' }));

    await waitFor(() => {
      expect(transport.loadProject).toHaveBeenCalledWith('/tmp/project.thoughttree');
      expect(useUIStore.getState().staleProjectSave).toBeNull();
    });
  });

  it('force-saves with a null base revision and dismisses the dialog', async () => {
    vi.mocked(transport.saveProject).mockResolvedValue('rev-9');
    useUIStore.getState().setStaleProjectSave({
      path: '/tmp/project.thoughttree',
      currentRevision: 'rev-2',
    });

    render(<StaleSaveDialog />);
    await userEvent.click(screen.getByRole('button', { name: 'Overwrite' }));

    await waitFor(() => {
      expect(transport.saveProject).toHaveBeenCalledWith(
        '/tmp/project.thoughttree',
        expect.any(String),
        null
      );
      expect(useGraphStore.getState().projectRevision).toBe('rev-9');
      expect(useUIStore.getState().staleProjectSave).toBeNull();
    });
  });
});
