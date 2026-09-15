import { useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useGraphStore } from '../../store/useGraphStore';
import { useUIStore } from '../../store/useUIStore';
import { getBackendTransport } from '../../lib/transport';
import { logger } from '../../lib/logger';

/** Right-click on a node, or on the empty pane (position already in flow coordinates). */
export type ContextMenuTarget =
  | { kind: 'node'; nodeId: string }
  | { kind: 'pane'; position: { x: number; y: number } };

interface ContextMenuProps {
  x: number;
  y: number;
  target: ContextMenuTarget;
  onClose: () => void;
}

export function ContextMenu({ x, y, target, onClose }: ContextMenuProps) {
  useEffect(() => {
    const handleClick = () => onClose();
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('click', handleClick);
    // Capture phase: the Palette stops propagation of Escape, but same-node
    // capture listeners still run, so the menu dismisses alongside it.
    window.addEventListener('keydown', handleEscape, true);
    return () => {
      window.removeEventListener('click', handleClick);
      window.removeEventListener('keydown', handleEscape, true);
    };
  }, [onClose]);

  return createPortal(
    <div
      className="context-menu"
      style={{ top: y, left: x }}
      onClick={(e) => e.stopPropagation()}
    >
      {target.kind === 'pane' ? (
        <PaneItems position={target.position} onClose={onClose} />
      ) : (
        <NodeItems nodeId={target.nodeId} onClose={onClose} />
      )}
    </div>,
    document.body
  );
}

function PaneItems({ position, onClose }: { position: { x: number; y: number }; onClose: () => void }) {
  const createUserNode = useGraphStore((state) => state.createUserNode);
  const linkVaultFile = useGraphStore((state) => state.linkVaultFile);
  const setNotice = useUIStore((state) => state.setNotice);

  const handleAddFile = async () => {
    onClose();
    try {
      const path = await getBackendTransport().pickVaultFile();
      if (path) await linkVaultFile(path, position);
    } catch (error) {
      // The picker rejects files outside the Vault with a human message.
      setNotice(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <>
      <button
        onClick={() => {
          createUserNode(position);
          onClose();
        }}
      >
        New note
      </button>
      <button onClick={() => void handleAddFile()}>Add file…</button>
    </>
  );
}

function NodeItems({ nodeId, onClose }: { nodeId: string; onClose: () => void }) {
  const createUserNodeDownstream = useGraphStore((state) => state.createUserNodeDownstream);
  const deleteNode = useGraphStore((state) => state.deleteNode);
  const acknowledgeFileChange = useGraphStore((state) => state.acknowledgeFileChange);
  const data = useGraphStore((state) => state.nodeData.get(nodeId));
  const isNodeBlocked = useGraphStore((state) => state.isNodeBlocked);
  const setNotice = useUIStore((state) => state.setNotice);
  const isAgent = data?.role === 'assistant';
  const canReply = isAgent && !isNodeBlocked(nodeId);
  const filePath = data?.role === 'file' ? data.path : null;

  const handleCopyPath = async () => {
    onClose();
    if (!filePath) return;
    try {
      await navigator.clipboard.writeText(filePath);
    } catch (error) {
      logger.error('Failed to copy path:', error);
      setNotice('Could not copy the path to the clipboard.');
    }
  };

  return (
    <>
      {isAgent && (
        <button
          onClick={() => {
            if (canReply) {
              createUserNodeDownstream(nodeId);
            }
            onClose();
          }}
          disabled={!canReply}
        >
          Reply
        </button>
      )}
      {filePath && (
        <>
          <button
            onClick={() => {
              void acknowledgeFileChange(nodeId);
              onClose();
            }}
          >
            Reload file
          </button>
          <button onClick={() => void handleCopyPath()}>Copy path</button>
        </>
      )}
      <button
        onClick={() => {
          deleteNode(nodeId);
          onClose();
        }}
      >
        Delete
      </button>
    </>
  );
}
