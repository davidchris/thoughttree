import { useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useGraphStore } from '../../store/useGraphStore';

interface ContextMenuProps {
  x: number;
  y: number;
  nodeId: string;
  onClose: () => void;
}

export function ContextMenu({ x, y, nodeId, onClose }: ContextMenuProps) {
  const { createUserNodeDownstream, deleteNode, nodeData, isNodeBlocked } = useGraphStore();
  const data = nodeData.get(nodeId);
  const isAgent = data?.role === 'assistant';
  const canReply = isAgent && !isNodeBlocked(nodeId);

  useEffect(() => {
    const handleClick = () => onClose();
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('click', handleClick);
    window.addEventListener('keydown', handleEscape);
    return () => {
      window.removeEventListener('click', handleClick);
      window.removeEventListener('keydown', handleEscape);
    };
  }, [onClose]);

  return createPortal(
    <div
      className="context-menu"
      style={{ top: y, left: x }}
      onClick={(e) => e.stopPropagation()}
    >
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
      <button
        onClick={() => {
          deleteNode(nodeId);
          onClose();
        }}
      >
        Delete
      </button>
    </div>,
    document.body
  );
}
