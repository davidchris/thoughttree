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
    saveProjectCopy: vi.fn(),
    snapshotProject: vi.fn().mockResolvedValue('snapshot-1'),
    listProjectRecovery: vi.fn().mockResolvedValue([]),
    readProjectRecovery: vi.fn(),
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

  it('compares versions without replacing the unsaved graph or its loaded revision', async () => {
    const state = useGraphStore.getState();
    const id = state.createUserNode();
    state.updateNodeContent(id, 'Unsaved message');
    const graph = useGraphStore.getState().graph;
    vi.mocked(transport.loadProject).mockResolvedValue({ data: '{"external":true}', revision: 'rev-2' });
    useUIStore.getState().setStaleProjectSave({ path: '/tmp/project.thoughttree', currentRevision: 'rev-2' });
    render(<StaleSaveDialog />);
    await userEvent.click(screen.getByRole('button', { name: 'Compare Versions' }));
    await screen.findByRole('textbox', { name: 'Disk version at comparison' });
    expect((screen.getByRole('textbox', { name: 'Unsaved version at comparison' }) as HTMLTextAreaElement).value).toContain('Unsaved message');
    expect(useGraphStore.getState().graph).toBe(graph);
    expect(useGraphStore.getState().projectRevision).toBe('rev-1');
    expect(transport.saveProject).not.toHaveBeenCalled();
  });

  it('saves a separate copy and leaves the original file alone', async () => {
    vi.mocked(transport.saveProjectCopy).mockResolvedValue(['/tmp/copy.thoughttree', 'rev-9']);
    useUIStore.getState().setStaleProjectSave({
      path: '/tmp/project.thoughttree',
      currentRevision: 'rev-2',
    });

    render(<StaleSaveDialog />);
    await userEvent.click(screen.getByRole('button', { name: 'Save a Separate Copy' }));

    await waitFor(() => {
      expect(transport.saveProjectCopy).toHaveBeenCalledWith('/tmp/project.thoughttree', expect.any(String));
      expect(transport.saveProject).not.toHaveBeenCalled();
      expect(useGraphStore.getState().projectRevision).toBe('rev-9');
      expect(useUIStore.getState().staleProjectSave).toBeNull();
    });
  });
});
