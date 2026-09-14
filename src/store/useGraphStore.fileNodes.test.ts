import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GRAPH_JSON_VERSION, GraphSerialize } from '@thoughttree/graph-model';
import type { BackendTransport, VaultFileStatus } from '../lib/transport';
import { setBackendTransport } from '../lib/transport';
import { createMockTransport } from '../test/mockTransport';
import { useGraphStore } from './useGraphStore';
import { useUIStore } from './useUIStore';

const MB = 1024 * 1024;

const NOTE = {
  path: 'notes/plan.md',
  name: 'plan.md',
  mimeType: 'text/markdown',
  size: 120,
  seenMtime: 1_000,
  seenSize: 120,
};

const IMAGE = {
  path: 'img/diagram.png',
  name: 'diagram.png',
  mimeType: 'image/png',
  size: 2 * MB,
  seenMtime: 5_000,
  seenSize: 2 * MB,
};

function okStatus(modifiedEpochMs: number, size: number, extra: Partial<Extract<VaultFileStatus, { status: 'ok' }>> = {}): VaultFileStatus {
  return { status: 'ok', stat: { size, modifiedEpochMs }, mimeType: 'text/markdown', name: 'plan.md', ...extra };
}

describe('useGraphStore file node status', () => {
  let transport: BackendTransport;

  beforeEach(() => {
    transport = createMockTransport();
    setBackendTransport(transport);
    useGraphStore.getState().newProject();
    useUIStore.getState().reset();
  });

  describe('refreshFileNodeStat', () => {
    it('reports ok when the file on disk matches the seen mtime and size', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(okStatus(1_000, 120));
      const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });

      await useGraphStore.getState().refreshFileNodeStat(id);

      expect(useGraphStore.getState().fileNodeStatus.get(id)).toEqual({
        state: 'ok',
        stat: { size: 120, modifiedEpochMs: 1_000 },
      });
      expect(transport.statVaultFile).toHaveBeenCalledWith('notes/plan.md');
    });

    it('reports changed when the mtime on disk differs from the seen mtime', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(okStatus(2_000, 120));
      const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });

      await useGraphStore.getState().refreshFileNodeStat(id);

      expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('changed');
    });

    it('reports changed when only the size differs', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(okStatus(1_000, 121));
      const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });

      await useGraphStore.getState().refreshFileNodeStat(id);

      expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('changed');
    });

    it('reports missing and invalid straight from the backend status', async () => {
      const missingId = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });
      const invalidId = useGraphStore.getState().addFileNode({ ...NOTE, path: 'other.md' }, { x: 0, y: 0 });
      vi.mocked(transport.statVaultFile).mockImplementation(async (path) =>
        path === 'notes/plan.md' ? { status: 'missing' } : { status: 'invalid' }
      );

      await useGraphStore.getState().refreshFileNodeStat(missingId);
      await useGraphStore.getState().refreshFileNodeStat(invalidId);

      const status = useGraphStore.getState().fileNodeStatus;
      expect(status.get(missingId)).toEqual({ state: 'missing' });
      expect(status.get(invalidId)).toEqual({ state: 'invalid' });
    });

    it('reports too-large for an image over the backend image limit, fetching limits once', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(
        okStatus(5_000, 6 * MB, { mimeType: 'image/png', name: 'diagram.png' })
      );
      const id = useGraphStore.getState().addFileNode({ ...IMAGE, size: 6 * MB, seenSize: 6 * MB }, { x: 0, y: 0 });

      await useGraphStore.getState().refreshFileNodeStat(id);
      await useGraphStore.getState().refreshFileNodeStat(id);

      expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('too-large');
      expect(transport.getAttachmentLimits).toHaveBeenCalledTimes(1);
    });

    it('never reports too-large for a non-image file, whatever its size', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(okStatus(1_000, 50 * MB));
      const id = useGraphStore.getState().addFileNode({ ...NOTE, size: 50 * MB, seenSize: 50 * MB }, { x: 0, y: 0 });

      await useGraphStore.getState().refreshFileNodeStat(id);

      expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('ok');
    });

    it('treats a non-raster image (svg) as a pointer: 6 MB is ok, never too-large', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(
        okStatus(5_000, 6 * MB, { mimeType: 'image/svg+xml', name: 'logo.svg' })
      );
      const id = useGraphStore.getState().addFileNode(
        { path: 'img/logo.svg', name: 'logo.svg', mimeType: 'image/svg+xml', size: 6 * MB, seenMtime: 5_000, seenSize: 6 * MB },
        { x: 0, y: 0 }
      );

      await useGraphStore.getState().refreshFileNodeStat(id);

      expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('ok');
      expect(transport.getAttachmentLimits).not.toHaveBeenCalled();
    });

    it('keeps a preview-refused too-large image too-large while the file is unchanged', async () => {
      // A 2 MB png is under the byte limit; only the preview (header parse) can
      // reveal that its longest side is over the limit.
      vi.mocked(transport.statVaultFile).mockResolvedValue(
        okStatus(5_000, 2 * MB, { mimeType: 'image/png', name: 'diagram.png' })
      );
      const id = useGraphStore.getState().addFileNode(IMAGE, { x: 0, y: 0 });
      await useGraphStore.getState().refreshFileNodeStat(id);
      expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('ok');

      useGraphStore.getState().markFileNodeTooLarge(id);
      await useGraphStore.getState().refreshFileNodeStat(id);

      expect(useGraphStore.getState().fileNodeStatus.get(id)).toEqual({
        state: 'too-large',
        stat: { size: 2 * MB, modifiedEpochMs: 5_000 },
      });
    });

    it('re-classifies a preview-refused image once the file changes on disk', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(
        okStatus(5_000, 2 * MB, { mimeType: 'image/png', name: 'diagram.png' })
      );
      const id = useGraphStore.getState().addFileNode(IMAGE, { x: 0, y: 0 });
      await useGraphStore.getState().refreshFileNodeStat(id);
      useGraphStore.getState().markFileNodeTooLarge(id);
      vi.mocked(transport.statVaultFile).mockResolvedValue(
        okStatus(6_000, 1 * MB, { mimeType: 'image/png', name: 'diagram.png' })
      );

      await useGraphStore.getState().refreshFileNodeStat(id);

      expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('changed');
    });

    it('adopts the first stat after a preview refusal as the refused version', async () => {
      // The card stats and previews in parallel; the refusal may land first.
      vi.mocked(transport.statVaultFile).mockResolvedValue(
        okStatus(5_000, 2 * MB, { mimeType: 'image/png', name: 'diagram.png' })
      );
      const id = useGraphStore.getState().addFileNode(IMAGE, { x: 0, y: 0 });
      useGraphStore.getState().markFileNodeTooLarge(id);

      await useGraphStore.getState().refreshFileNodeStat(id);

      expect(useGraphStore.getState().fileNodeStatus.get(id)).toEqual({
        state: 'too-large',
        stat: { size: 2 * MB, modifiedEpochMs: 5_000 },
      });
    });

    it('ignores nodes that are not file nodes', async () => {
      const id = useGraphStore.getState().createUserNode();

      await useGraphStore.getState().refreshFileNodeStat(id);

      expect(transport.statVaultFile).not.toHaveBeenCalled();
      expect(useGraphStore.getState().fileNodeStatus.has(id)).toBe(false);
    });
  });

  describe('refreshAllFileNodeStats', () => {
    it('stats every file node and leaves other nodes alone', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(okStatus(1_000, 120));
      const state = useGraphStore.getState();
      const a = state.addFileNode(NOTE, { x: 0, y: 0 });
      const b = state.addFileNode({ ...NOTE, path: 'b.md', name: 'b.md' }, { x: 0, y: 0 });
      state.createUserNode();

      await useGraphStore.getState().refreshAllFileNodeStats();

      expect(transport.statVaultFile).toHaveBeenCalledTimes(2);
      expect(useGraphStore.getState().fileNodeStatus.get(a)?.state).toBe('ok');
      expect(useGraphStore.getState().fileNodeStatus.get(b)?.state).toBe('ok');
    });
  });

  describe('acknowledgeFileChange', () => {
    it('adopts the on-disk mtime and size, marks the project dirty and clears the changed state', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(okStatus(2_000, 130));
      const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });
      await useGraphStore.getState().refreshFileNodeStat(id);
      useGraphStore.setState({ isDirty: false });

      await useGraphStore.getState().acknowledgeFileChange(id);

      const node = useGraphStore.getState().nodeData.get(id);
      expect(node).toMatchObject({ role: 'file', seenMtime: 2_000, seenSize: 130, size: 130 });
      expect(useGraphStore.getState().isDirty).toBe(true);
      expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('ok');
    });

    it('stats the file first when no status is known yet', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(okStatus(3_000, 140));
      const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });

      await useGraphStore.getState().acknowledgeFileChange(id);

      expect(useGraphStore.getState().nodeData.get(id)).toMatchObject({ seenMtime: 3_000, seenSize: 140 });
    });

    it('cannot acknowledge a missing file', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue({ status: 'missing' });
      const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });
      await useGraphStore.getState().refreshFileNodeStat(id);

      await useGraphStore.getState().acknowledgeFileChange(id);

      expect(useGraphStore.getState().nodeData.get(id)).toMatchObject({ seenMtime: 1_000, seenSize: 120 });
      expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('missing');
    });
  });

  describe('sendBlocker', () => {
    it('returns null when the lineage has no file nodes', () => {
      const userId = useGraphStore.getState().createUserNode();

      expect(useGraphStore.getState().sendBlocker(userId)).toBeNull();
    });

    it('returns null when every file node in the lineage is ok or merely changed', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(okStatus(2_000, 120));
      const state = useGraphStore.getState();
      const fileId = state.addFileNode(NOTE, { x: 0, y: 0 });
      const userId = state.createUserNodeDownstream(fileId);
      await useGraphStore.getState().refreshFileNodeStat(fileId);

      expect(useGraphStore.getState().sendBlocker(userId)).toBeNull();
    });

    it('names a missing ancestor file, even several hops up', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue({ status: 'missing' });
      const state = useGraphStore.getState();
      const fileId = state.addFileNode(NOTE, { x: 0, y: 0 });
      const firstUser = state.createUserNodeDownstream(fileId);
      const agent = state.createAgentNodeDownstream(firstUser);
      state.stopStreaming(agent);
      const secondUser = state.createUserNodeDownstream(agent);
      await useGraphStore.getState().refreshFileNodeStat(fileId);

      const reason = useGraphStore.getState().sendBlocker(secondUser);

      expect(reason).toContain('plan.md');
      expect(reason).toMatch(/missing/i);
    });

    it('names an over-limit image and an invalid path', async () => {
      const state = useGraphStore.getState();
      const imageId = state.addFileNode({ ...IMAGE, size: 6 * MB, seenSize: 6 * MB }, { x: 0, y: 0 });
      const invalidId = state.addFileNode({ ...NOTE, path: 'bad.md', name: 'bad.md' }, { x: 0, y: 0 });
      const userA = state.createUserNodeDownstream(imageId);
      const userB = state.createUserNodeDownstream(invalidId);
      vi.mocked(transport.statVaultFile).mockImplementation(async (path) =>
        path === 'img/diagram.png'
          ? okStatus(5_000, 6 * MB, { mimeType: 'image/png', name: 'diagram.png' })
          : { status: 'invalid' }
      );
      await useGraphStore.getState().refreshAllFileNodeStats();

      expect(useGraphStore.getState().sendBlocker(userA)).toMatch(/diagram\.png.*too large/i);
      expect(useGraphStore.getState().sendBlocker(userB)).toMatch(/bad\.md/);
    });

    it('ignores file nodes outside the lineage', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue({ status: 'missing' });
      const state = useGraphStore.getState();
      const fileId = state.addFileNode(NOTE, { x: 0, y: 0 });
      state.createUserNodeDownstream(fileId);
      const unrelatedUser = state.createUserNode();
      await useGraphStore.getState().refreshFileNodeStat(fileId);

      expect(useGraphStore.getState().sendBlocker(unrelatedUser)).toBeNull();
    });

    it('blocks sending once the preview refused an image as too large, before any ACP call', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(
        okStatus(5_000, 2 * MB, { mimeType: 'image/png', name: 'diagram.png' })
      );
      const state = useGraphStore.getState();
      const imageId = state.addFileNode(IMAGE, { x: 0, y: 0 });
      const userId = state.createUserNodeDownstream(imageId);
      await useGraphStore.getState().refreshFileNodeStat(imageId);
      expect(useGraphStore.getState().sendBlocker(userId)).toBeNull();

      useGraphStore.getState().markFileNodeTooLarge(imageId);

      expect(useGraphStore.getState().sendBlocker(userId)).toMatch(/diagram\.png.*too large/i);
    });
  });

  describe('canGenerate', () => {
    it('is false for an empty user node with nothing in its lineage', () => {
      const userId = useGraphStore.getState().createUserNode();

      expect(useGraphStore.getState().canGenerate(userId)).toBe(false);
    });

    it('is true for text, and for an inline image without text', () => {
      const state = useGraphStore.getState();
      const textUser = state.createUserNode();
      state.updateNodeContent(textUser, 'hello');
      const imageUser = useGraphStore.getState().createUserNode();
      useGraphStore.getState().addNodeImage(imageUser, { data: 'AAAA', mimeType: 'image/png' });

      expect(useGraphStore.getState().canGenerate(textUser)).toBe(true);
      expect(useGraphStore.getState().canGenerate(imageUser)).toBe(true);
    });

    it('lets a file node stand alone: an empty user node below it sends one file-only user message', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(okStatus(1_000, 120));
      const state = useGraphStore.getState();
      const fileId = state.addFileNode(NOTE, { x: 0, y: 0 });
      const userId = state.createUserNodeDownstream(fileId);
      await useGraphStore.getState().refreshFileNodeStat(fileId);

      expect(useGraphStore.getState().canGenerate(userId)).toBe(true);
      expect(useGraphStore.getState().buildConversationContext(userId)).toEqual([
        {
          role: 'user',
          content: '',
          files: [{ path: 'notes/plan.md', name: 'plan.md', mimeType: 'text/markdown', size: 120 }],
        },
      ]);
    });

    it('is false while a lineage file node blocks sending, even with text', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue({ status: 'missing' });
      const state = useGraphStore.getState();
      const fileId = state.addFileNode(NOTE, { x: 0, y: 0 });
      const userId = state.createUserNodeDownstream(fileId);
      state.updateNodeContent(userId, 'Summarise the plan');
      await useGraphStore.getState().refreshFileNodeStat(fileId);

      expect(useGraphStore.getState().canGenerate(userId)).toBe(false);
    });

    it('is false for anything that is not a user node', () => {
      const fileId = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });

      expect(useGraphStore.getState().canGenerate(fileId)).toBe(false);
      expect(useGraphStore.getState().canGenerate('nope')).toBe(false);
    });
  });

  describe('linkVaultFile', () => {
    it('stats the file and adds a file node carrying the backend name, mime type and stat', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(
        okStatus(7_000, 300, { mimeType: 'text/plain', name: 'todo.txt' })
      );

      const id = await useGraphStore.getState().linkVaultFile('inbox/todo.txt', { x: 40, y: 50 });

      expect(id).not.toBeNull();
      expect(useGraphStore.getState().nodeData.get(id!)).toMatchObject({
        role: 'file',
        path: 'inbox/todo.txt',
        name: 'todo.txt',
        mimeType: 'text/plain',
        size: 300,
        seenMtime: 7_000,
        seenSize: 300,
      });
      expect(useGraphStore.getState().graph.layout.get(id!)).toEqual({ x: 40, y: 50 });
      expect(useGraphStore.getState().fileNodeStatus.get(id!)?.state).toBe('ok');
    });

    it('adds nothing and posts a notice when the file is missing or invalid', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue({ status: 'missing' });

      const id = await useGraphStore.getState().linkVaultFile('gone.md', { x: 0, y: 0 });

      expect(id).toBeNull();
      expect(useGraphStore.getState().graph.nodes.size).toBe(0);
      expect(useUIStore.getState().notice).toMatch(/gone\.md/);
    });
  });

  describe('status lifecycle', () => {
    it('drops the status when the node is deleted or removed via ReactFlow', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(okStatus(1_000, 120));
      const state = useGraphStore.getState();
      const a = state.addFileNode(NOTE, { x: 0, y: 0 });
      const b = state.addFileNode({ ...NOTE, path: 'b.md', name: 'b.md' }, { x: 0, y: 0 });
      await useGraphStore.getState().refreshAllFileNodeStats();

      useGraphStore.getState().deleteNode(a);
      useGraphStore.getState().onNodesChange([{ type: 'remove', id: b }]);

      expect(useGraphStore.getState().fileNodeStatus.size).toBe(0);
    });

    it('clears statuses on newProject', async () => {
      vi.mocked(transport.statVaultFile).mockResolvedValue(okStatus(1_000, 120));
      const id = useGraphStore.getState().addFileNode(NOTE, { x: 0, y: 0 });
      await useGraphStore.getState().refreshFileNodeStat(id);

      useGraphStore.getState().newProject();

      expect(useGraphStore.getState().fileNodeStatus.size).toBe(0);
    });

    it('never persists the status and re-stats file nodes after loading a project', async () => {
      const draft = useGraphStore.getState();
      draft.addFileNode(NOTE, { x: 0, y: 0 });
      const graphJson = GraphSerialize.toJSON(useGraphStore.getState().graph);
      useGraphStore.getState().newProject();
      vi.mocked(transport.loadProject).mockResolvedValue({
        data: JSON.stringify({ version: GRAPH_JSON_VERSION, graph: graphJson }),
        revision: 'rev-1',
      });
      vi.mocked(transport.statVaultFile).mockResolvedValue(okStatus(9_000, 120));

      await useGraphStore.getState().loadProject('/vault/p.thoughttree');
      await vi.waitFor(() => expect(transport.statVaultFile).toHaveBeenCalledWith('notes/plan.md'));

      expect(JSON.stringify(graphJson)).not.toContain('fileNodeStatus');
      const [id] = [...useGraphStore.getState().graph.nodes.keys()];
      await vi.waitFor(() =>
        expect(useGraphStore.getState().fileNodeStatus.get(id)?.state).toBe('changed')
      );
    });
  });
});
