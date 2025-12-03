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
      <div className="file-autocomplete" style={{ top: position.top, left: position.left }}>
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
