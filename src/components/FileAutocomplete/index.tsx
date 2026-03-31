import { useState, useEffect, useRef, useCallback, forwardRef, useImperativeHandle } from 'react';
import { createPortal } from 'react-dom';
import { searchFiles } from '../../lib/tauri';
import './styles.css';

export interface FileAutocompleteProps {
  isOpen: boolean;
  query: string;
  position: { top: number; left: number };
  onSelect: (filePath: string) => void;
  onClose: () => void;
}

export interface FileAutocompleteRef {
  handleKeyDown: (e: KeyboardEvent) => boolean;
}

export const FileAutocomplete = forwardRef<FileAutocompleteRef, FileAutocompleteProps>(
  function FileAutocomplete({ isOpen, query, position, onSelect, onClose }, ref) {
    const [files, setFiles] = useState<string[]>([]);
    const [selectedIndex, setSelectedIndex] = useState(0);
    const [isLoading, setIsLoading] = useState(false);
    const listRef = useRef<HTMLDivElement>(null);
    const dropdownRef = useRef<HTMLDivElement>(null);
    const [adjustedPosition, setAdjustedPosition] = useState(position);

    // Debounced search
    useEffect(() => {
      console.log('[FileAutocomplete] useEffect triggered, isOpen:', isOpen, 'query:', query);
      if (!isOpen) {
        setFiles([]);
        return;
      }

      setIsLoading(true);
      const timeoutId = setTimeout(async () => {
        try {
          console.log('[FileAutocomplete] Calling searchFiles with query:', query);
          const results = await searchFiles(query, 15);
          console.log('[FileAutocomplete] Search results:', results);
          setFiles(results);
          setSelectedIndex(0);
        } catch (error) {
          console.error('[FileAutocomplete] File search failed:', error);
          setFiles([]);
        } finally {
          setIsLoading(false);
        }
      }, 100);

      return () => clearTimeout(timeoutId);
    }, [isOpen, query]);

    // Scroll selected item into view
    useEffect(() => {
      if (listRef.current && files.length > 0) {
        const selected = listRef.current.children[selectedIndex] as HTMLElement;
        selected?.scrollIntoView({ block: 'nearest' });
      }
    }, [selectedIndex, files.length]);

    // Adjust position to stay within viewport
    useEffect(() => {
      if (!isOpen || !dropdownRef.current) {
        setAdjustedPosition(position);
        return;
      }

      const dropdown = dropdownRef.current;
      const dropdownHeight = dropdown.offsetHeight;
      const dropdownWidth = dropdown.offsetWidth;
      const viewportHeight = window.innerHeight;
      const viewportWidth = window.innerWidth;
      const PADDING = 8;

      let { top, left } = position;

      // Flip above cursor if it would overflow the bottom
      if (top + dropdownHeight + PADDING > viewportHeight) {
        // position.top is already caret.top + caret.height + 4, so go back above the caret line
        top = position.top - dropdownHeight - 8;
      }

      // Clamp left edge
      if (left + dropdownWidth > viewportWidth - PADDING) {
        left = viewportWidth - dropdownWidth - PADDING;
      }

      setAdjustedPosition({ top, left });
    }, [isOpen, position, files.length]);

    // Keyboard navigation handler exposed via ref
    const handleKeyDown = useCallback(
      (e: KeyboardEvent): boolean => {
        if (!isOpen || files.length === 0) {
          if (e.key === 'Escape') {
            onClose();
            return true;
          }
          return false;
        }

        switch (e.key) {
          case 'ArrowDown':
            setSelectedIndex((i) => Math.min(i + 1, files.length - 1));
            return true;
          case 'ArrowUp':
            setSelectedIndex((i) => Math.max(i - 1, 0));
            return true;
          case 'Enter':
          case 'Tab':
            if (files[selectedIndex]) {
              onSelect(files[selectedIndex]);
            }
            return true;
          case 'Escape':
            onClose();
            return true;
          default:
            return false;
        }
      },
      [isOpen, files, selectedIndex, onSelect, onClose]
    );

    useImperativeHandle(ref, () => ({ handleKeyDown }), [handleKeyDown]);

    console.log('[FileAutocomplete] Rendering, isOpen:', isOpen, 'files:', files.length, 'position:', position);

    if (!isOpen) return null;

    // Use portal to escape ReactFlow's transform context
    return createPortal(
      <div ref={dropdownRef} className="file-autocomplete" style={{ top: adjustedPosition.top, left: adjustedPosition.left }}>
        <div className="file-autocomplete-list" ref={listRef}>
          {isLoading && <div className="file-autocomplete-loading">Searching...</div>}
          {!isLoading && files.length === 0 && (
            <div className="file-autocomplete-empty">No files found</div>
          )}
          {files.map((file, index) => (
            <div
              key={file}
              className={`file-autocomplete-item ${index === selectedIndex ? 'selected' : ''}`}
              onClick={() => onSelect(file)}
              onMouseEnter={() => setSelectedIndex(index)}
            >
              <span className="file-path">{file}</span>
            </div>
          ))}
        </div>
      </div>,
      document.body
    );
  }
);
