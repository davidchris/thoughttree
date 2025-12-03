import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './SetupWizard.css';

interface SetupWizardProps {
  onComplete: () => void;
}

export function SetupWizard({ onComplete }: SetupWizardProps) {
  const [isSelecting, setIsSelecting] = useState(false);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleSelectDirectory = async () => {
    setIsSelecting(true);
    setError(null);
    try {
      const path = await invoke<string | null>('pick_notes_directory');
      if (path) {
        setSelectedPath(path);
      }
    } catch (e) {
      setError(`Failed to select directory: ${e}`);
    } finally {
      setIsSelecting(false);
    }
  };

  const handleConfirm = async () => {
    if (!selectedPath) return;

    try {
      await invoke('set_notes_directory', { path: selectedPath });
      onComplete();
    } catch (e) {
      setError(`Failed to save settings: ${e}`);
    }
  };

  return (
    <div className="setup-wizard-overlay">
      <div className="setup-wizard">
        <h1>Welcome to ThoughtTree</h1>
        <p className="setup-description">
          ThoughtTree is a thinking tool for brainstorming, research, and synthesis.
          Claude can read your notes to provide context-aware assistance.
        </p>

        <div className="setup-step">
          <h2>Select Your Notes Directory</h2>
          <p>
            Choose the folder where your notes are stored (e.g., Obsidian vault, Nextcloud Notes).
            ThoughtTree projects will be saved here, and Claude will be able to read files in this directory.
          </p>

          <div className="directory-selector">
            {selectedPath ? (
              <div className="selected-path">
                <span className="path-label">Selected:</span>
                <code>{selectedPath}</code>
              </div>
            ) : (
              <p className="no-path">No directory selected</p>
            )}

            <button
              onClick={handleSelectDirectory}
              disabled={isSelecting}
              className="select-button"
            >
              {isSelecting ? 'Selecting...' : 'Choose Directory'}
            </button>
          </div>

          {error && <p className="error-message">{error}</p>}
        </div>

        <div className="setup-actions">
          <button
            onClick={handleConfirm}
            disabled={!selectedPath}
            className="confirm-button"
          >
            Get Started
          </button>
        </div>
      </div>
    </div>
  );
}
