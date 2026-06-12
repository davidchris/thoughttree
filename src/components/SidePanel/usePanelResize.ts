import { useEffect, useState } from 'react';

const DEFAULT_WIDTH = 850; // ~100 character columns at 14px monospace
const MIN_WIDTH = 200;
const MAX_WIDTH_PERCENT = 0.8; // 80% of viewport

/** Drag-to-resize behavior for the side panel's left edge. */
export function usePanelResize() {
  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const [isResizing, setIsResizing] = useState(false);

  useEffect(() => {
    if (!isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      const maxWidth = window.innerWidth * MAX_WIDTH_PERCENT;
      const newWidth = Math.max(MIN_WIDTH, Math.min(maxWidth, window.innerWidth - e.clientX));
      setWidth(newWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
      document.body.classList.remove('resizing');
    };

    document.body.classList.add('resizing');
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizing]);

  const handleResizeStart = (e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
  };

  return { width, handleResizeStart };
}
