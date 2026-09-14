import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ReactFlowProvider } from '@xyflow/react';
import type { NodeProps } from '@xyflow/react';
import type { BackendTransport, FilePreviewResponse } from '../../lib/transport';
import { setBackendTransport } from '../../lib/transport';
import { createMockTransport } from '../../test/mockTransport';
import { useGraphStore } from '../../store/useGraphStore';
import { useUIStore } from '../../store/useUIStore';
import { resetFilePreviewCache } from '../../hooks/useFilePreview';
import { FileNode } from './FileNode';

const NOTE = {
  path: 'notes/plan.md',
  name: 'plan.md',
  mimeType: 'text/markdown',
  size: 2048,
  seenMtime: 1_000,
  seenSize: 2048,
};

const IMAGE = {
  path: 'img/diagram.png',
  name: 'diagram.png',
  mimeType: 'image/png',
  size: 300_000,
  seenMtime: 5_000,
  seenSize: 300_000,
};

function textPreview(excerpt: string, truncated = false): FilePreviewResponse {
  return {
    info: { name: 'plan.md', mimeType: 'text/markdown', size: 2048, modifiedEpochMs: 1_000 },
    preview: { kind: 'text', excerpt, truncated },
  };
}

function renderFileNode(id: string, selected = false) {
  const props = { id, selected } as unknown as NodeProps;
  return render(
    <ReactFlowProvider>
      <FileNode {...props} />
    </ReactFlowProvider>
  );
}

describe('FileNode', () => {
  let transport: BackendTransport;

  beforeEach(() => {
    transport = createMockTransport();
    setBackendTransport(transport);
    resetFilePreviewCache();
    useGraphStore.getState().newProject();
    useUIStore.getState().reset();
    vi.mocked(transport.statVaultFile).mockResolvedValue({
      status: 'ok',
      stat: { size: 2048, modifiedEpochMs: 1_000 },
      mimeType: 'text/markdown',
      name: 'plan.md',
    });
  });

  it('shows the type badge, name, size, a clipped text excerpt and the on-disk hint', async () => {
    vi.mocked(transport.readVaultFilePreview).mockResolvedValue(textPreview('# Plan\nStep one', true));
    const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });

    renderFileNode(id);

    expect(screen.getByText('MD', { selector: '.file-type-badge' })).toBeInTheDocument();
    expect(screen.getByText('plan.md')).toBeInTheDocument();
    expect(screen.getByText('2.0 KB')).toBeInTheDocument();
    expect(await screen.findByText(/# Plan/)).toHaveTextContent('…');
    expect(screen.getByText('agent reads from disk')).toBeInTheDocument();
    expect(transport.readVaultFilePreview).toHaveBeenCalledWith('notes/plan.md');
  });

  it('has a source handle only, never a target handle', () => {
    vi.mocked(transport.readVaultFilePreview).mockResolvedValue(textPreview(''));
    const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });

    const { container } = renderFileNode(id);

    expect(container.querySelector('.react-flow__handle.source')).not.toBeNull();
    expect(container.querySelector('.react-flow__handle.target')).toBeNull();
  });

  it('renders an image thumbnail from the preview without the on-disk hint', async () => {
    vi.mocked(transport.readVaultFilePreview).mockResolvedValue({
      info: { name: 'diagram.png', mimeType: 'image/png', size: 300_000, modifiedEpochMs: 5_000 },
      preview: { kind: 'image', data: 'AAAA', mimeType: 'image/png', width: 64, height: 32 },
    });
    const id = useGraphStore.getState().addFileNode(IMAGE, { x: 0, y: 0 });

    renderFileNode(id);

    const img = await screen.findByRole('img', { name: 'diagram.png' });
    expect(img).toHaveAttribute('src', 'data:image/png;base64,AAAA');
    expect(screen.getByText('PNG', { selector: '.file-type-badge' })).toBeInTheDocument();
    expect(screen.queryByText('agent reads from disk')).not.toBeInTheDocument();
  });

  it('shows the on-disk hint for a non-raster image such as svg, which goes as a pointer', async () => {
    vi.mocked(transport.statVaultFile).mockResolvedValue({
      status: 'ok',
      stat: { size: 6 * 1024 * 1024, modifiedEpochMs: 5_000 },
      mimeType: 'image/svg+xml',
      name: 'logo.svg',
    });
    vi.mocked(transport.readVaultFilePreview).mockResolvedValue(textPreview('<svg/>'));
    const id = useGraphStore.getState().addFileNode(
      { path: 'img/logo.svg', name: 'logo.svg', mimeType: 'image/svg+xml', size: 6 * 1024 * 1024, seenMtime: 5_000, seenSize: 6 * 1024 * 1024 },
      { x: 0, y: 0 }
    );

    renderFileNode(id);

    expect(await screen.findByText('agent reads from disk')).toBeInTheDocument();
    await waitFor(() => expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('ok'));
    expect(screen.queryByText('too large for the agent')).not.toBeInTheDocument();
  });

  it('flags a file changed on disk and Reload adopts the new version and reloads the preview', async () => {
    vi.mocked(transport.statVaultFile).mockResolvedValue({
      status: 'ok',
      stat: { size: 4096, modifiedEpochMs: 2_000 },
      mimeType: 'text/markdown',
      name: 'plan.md',
    });
    vi.mocked(transport.readVaultFilePreview)
      .mockResolvedValueOnce(textPreview('old text'))
      .mockResolvedValueOnce(textPreview('new text'));
    const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });

    renderFileNode(id);

    const flag = await screen.findByText('changed on disk');
    expect(flag).toHaveAttribute('title', 'Reload to update the preview and what the agent will see');
    await screen.findByText('old text');
    await userEvent.click(screen.getByRole('button', { name: 'Reload' }));

    expect(await screen.findByText('new text')).toBeInTheDocument();
    expect(screen.queryByText('changed on disk')).not.toBeInTheDocument();
    expect(useGraphStore.getState().nodeData.get(id)).toMatchObject({ seenMtime: 2_000, seenSize: 4096 });
    expect(transport.readVaultFilePreview).toHaveBeenCalledTimes(2);
  });

  it('renders the broken state for a missing file', async () => {
    vi.mocked(transport.statVaultFile).mockResolvedValue({ status: 'missing' });
    vi.mocked(transport.readVaultFilePreview).mockRejectedValue('missing: notes/plan.md');
    const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });

    const { container } = renderFileNode(id);

    expect(await screen.findByText('file missing')).toBeInTheDocument();
    await waitFor(() => expect(container.querySelector('.file-node')).toHaveClass('state-missing'));
  });

  it('renders the too-large state from a too_large: preview error even when the stat looks fine, and blocks sending', async () => {
    vi.mocked(transport.statVaultFile).mockResolvedValue({
      status: 'ok',
      stat: { size: 300_000, modifiedEpochMs: 5_000 },
      mimeType: 'image/png',
      name: 'diagram.png',
    });
    vi.mocked(transport.readVaultFilePreview).mockRejectedValue('too_large: 5242880');
    const id = useGraphStore.getState().addFileNode(IMAGE, { x: 0, y: 0 });
    const userId = useGraphStore.getState().createUserNodeDownstream(id);

    renderFileNode(id);

    expect(await screen.findByText('too large for the agent')).toBeInTheDocument();
    await waitFor(() => expect(useGraphStore.getState().sendBlocker(userId)).toMatch(/diagram\.png.*too large/i));
  });

  it('serves a second mount of the same file version from the preview cache', async () => {
    vi.mocked(transport.readVaultFilePreview).mockResolvedValue(textPreview('cached'));
    const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });
    const first = renderFileNode(id);
    await screen.findByText('cached');
    first.unmount();

    renderFileNode(id);

    expect(await screen.findByText('cached')).toBeInTheDocument();
    expect(transport.readVaultFilePreview).toHaveBeenCalledTimes(1);
  });

  it('opens the side panel preview on double click', async () => {
    vi.mocked(transport.readVaultFilePreview).mockResolvedValue(textPreview(''));
    const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });

    renderFileNode(id);
    await userEvent.dblClick(screen.getByText('plan.md'));

    expect(useUIStore.getState().previewNodeId).toBe(id);
  });
});
