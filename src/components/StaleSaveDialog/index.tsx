import { useEffect, useState } from 'react';
import { useGraphStore } from '../../store/useGraphStore';
import { useUIStore } from '../../store/useUIStore';
import { logger } from '../../lib/logger';
import './styles.css';

export function StaleSaveDialog() {
  const conflict = useUIStore((state) => state.staleProjectSave);
  const setConflict = useUIStore((state) => state.setStaleProjectSave);
  const projectPath = useGraphStore((state) => state.projectPath);
  const loadProject = useGraphStore((state) => state.loadProject);
  const saveProject = useGraphStore((state) => state.saveProject);
  const [busyAction, setBusyAction] = useState<'reload' | 'overwrite' | null>(null);

  useEffect(() => {
    if (conflict && projectPath !== conflict.path) {
      setConflict(null);
    }
  }, [conflict, projectPath, setConflict]);

  if (!conflict) {
    return null;
  }

  const projectName = conflict.path.split('/').pop() ?? conflict.path;

  const handleReload = async () => {
    setBusyAction('reload');
    try {
      await loadProject(conflict.path);
      setConflict(null);
    } catch (error) {
      logger.error('Failed to reload project after stale save:', error);
    } finally {
      setBusyAction(null);
    }
  };

  const handleOverwrite = async () => {
    setBusyAction('overwrite');
    try {
      await saveProject({ force: true });
      setConflict(null);
    } catch (error) {
      logger.error('Failed to overwrite project after stale save:', error);
    } finally {
      setBusyAction(null);
    }
  };

  return (
    <div className="stale-save-overlay">
      <div className="stale-save-dialog" role="dialog" aria-modal="true" aria-label="Stale save">
        <div className="stale-save-header">
          <h2>File Changed on Disk</h2>
          <span className="stale-save-file">{projectName}</span>
        </div>

        <div className="stale-save-content">
          <p>Your save was rejected because this project changed after you loaded it.</p>
          <p>Reload discards your local edits. Overwrite force-saves your current graph.</p>
        </div>

        <div className="stale-save-actions">
          <button onClick={handleReload} disabled={busyAction !== null} className="secondary">
            {busyAction === 'reload' ? 'Reloading...' : 'Reload'}
          </button>
          <button onClick={handleOverwrite} disabled={busyAction !== null} className="primary">
            {busyAction === 'overwrite' ? 'Overwriting...' : 'Overwrite'}
          </button>
        </div>
      </div>
    </div>
  );
}
