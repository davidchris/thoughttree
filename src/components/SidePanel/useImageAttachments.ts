import { useCallback, useState } from 'react';
import { useGraphStore } from '../../store/useGraphStore';
import { resizeIfNeeded, fileToBase64 } from '../../lib/imageUtils';
import type { ImageAttachment } from '../../types';
import { logger } from '../../lib/logger';

/** Paste and drag-and-drop image attachment handling for a node. */
export function useImageAttachments(nodeId: string | null) {
  const addNodeImage = useGraphStore((state) => state.addNodeImage);
  const removeNodeImage = useGraphStore((state) => state.removeNodeImage);
  const [isDragOver, setIsDragOver] = useState(false);

  const processImageFile = useCallback(
    async (file: File) => {
      if (!file.type.startsWith('image/') || !nodeId) return;

      try {
        const resized = await resizeIfNeeded(file);
        const base64 = await fileToBase64(resized);
        const image: ImageAttachment = {
          data: base64,
          mimeType: file.type,
          name: file.name,
        };
        addNodeImage(nodeId, image);
      } catch (error) {
        logger.error('Failed to process image:', error);
      }
    },
    [nodeId, addNodeImage]
  );

  const handlePaste = useCallback(
    async (e: React.ClipboardEvent) => {
      const items = e.clipboardData?.items;
      if (!items) return;

      for (const item of Array.from(items)) {
        if (item.type.startsWith('image/')) {
          e.preventDefault();
          const file = item.getAsFile();
          if (file) {
            await processImageFile(file);
          }
        }
      }
    },
    [processImageFile]
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer.types.includes('Files')) {
      setIsDragOver(true);
    }
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    // Leaving to a child element doesn't end the drag
    const relatedTarget = e.relatedTarget as Node | null;
    if (relatedTarget && e.currentTarget.contains(relatedTarget)) {
      return;
    }
    setIsDragOver(false);
  }, []);

  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setIsDragOver(false);

      const files = e.dataTransfer.files;
      for (const file of Array.from(files)) {
        if (file.type.startsWith('image/')) {
          await processImageFile(file);
        }
      }
    },
    [processImageFile]
  );

  const handleRemoveImage = useCallback(
    (index: number, e: React.MouseEvent) => {
      e.stopPropagation();
      if (nodeId) {
        removeNodeImage(nodeId, index);
      }
    },
    [nodeId, removeNodeImage]
  );

  return {
    isDragOver,
    handlePaste,
    handleDragOver,
    handleDragLeave,
    handleDrop,
    handleRemoveImage,
  };
}
