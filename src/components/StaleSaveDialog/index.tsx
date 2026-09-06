import { useEffect, useState } from 'react';
import { useGraphStore } from '../../store/useGraphStore';
import { useUIStore } from '../../store/useUIStore';
import { getBackendTransport } from '../../lib/transport';
import './styles.css';

export function StaleSaveDialog() {
  const conflict = useUIStore((state) => state.staleProjectSave);
  const setConflict = useUIStore((state) => state.setStaleProjectSave);
  const projectPath = useGraphStore((state) => state.projectPath);
  const streaming = useGraphStore((state) => state.streamingNodeIds.size > 0);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [versions, setVersions] = useState<{ local: string; disk: string } | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setVersions(null);
    setError(null);
    if (conflict && projectPath !== conflict.path) setConflict(null);
  }, [conflict, projectPath, setConflict]);

  if (!conflict) return null;

  const run = async (action: string, operation: () => Promise<void>) => {
    setBusyAction(action);
    setError(null);
    try { await operation(); }
    catch (error) { setError(error instanceof Error ? error.message : 'The action failed. Your unsaved graph is still open.'); }
    finally { setBusyAction(null); }
  };

  const compare = async () => {
    const disk = await getBackendTransport().loadProject(conflict.path);
    // Comparison is read-only. It never adopts the disk revision for saving.
    setVersions({ local: useGraphStore.getState().projectContent(), disk: disk.data });
  };

  return (
    <div className="stale-save-overlay">
      <div className={`stale-save-dialog ${versions ? 'with-comparison' : ''}`} role="dialog" aria-modal="true" aria-label="Stale save">
        <div className="stale-save-header">
          <h2>File Changed on Disk</h2>
          <span className="stale-save-file">{conflict.path.split(/[\\/]/).pop()}</span>
        </div>
        <div className="stale-save-content">
          <p>Your save was rejected. Your unsaved graph is still open.</p>
          <p>Reload first preserves your edits in Recovery snapshots, then opens the disk version. Save a Separate Copy keeps both Projects.</p>
          {streaming && <p>Wait for the current response to finish before reloading.</p>}
          {error && <p role="alert">{error}</p>}
          {versions && <div className="project-comparison">
            <label>Unsaved version at comparison<textarea readOnly value={JSON.stringify(JSON.parse(versions.local), null, 2)} /></label>
            <label>Disk version at comparison<textarea readOnly value={formatVersion(versions.disk)} /></label>
          </div>}
        </div>
        <div className="stale-save-actions">
          <button onClick={() => void run('compare', compare)} disabled={busyAction !== null} className="secondary">Compare Versions</button>
          <button onClick={() => void run('reload', () => useGraphStore.getState().loadProject(conflict.path))} disabled={busyAction !== null || streaming} className="secondary">
            {busyAction === 'reload' ? 'Preserving and reloading...' : 'Reload'}
          </button>
          <button onClick={() => void run('copy', () => useGraphStore.getState().saveProjectCopy())} disabled={busyAction !== null} className="primary">
            {busyAction === 'copy' ? 'Saving copy...' : 'Save a Separate Copy'}
          </button>
        </div>
      </div>
    </div>
  );
}

function formatVersion(content: string) {
  try { return JSON.stringify(JSON.parse(content), null, 2); }
  catch { return content; }
}
