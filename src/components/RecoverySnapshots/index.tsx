import { useState } from 'react';
import { getBackendTransport, type RecoveryEntry } from '../../lib/transport';
import { useGraphStore } from '../../store/useGraphStore';
import { ToolbarButton } from '../Toolbar/ToolbarButton';
import { HistoryIcon } from '../Toolbar/ToolbarIcons';
import '../StaleSaveDialog/styles.css';

export function RecoverySnapshots() {
  const [open, setOpen] = useState(false);
  const [entries, setEntries] = useState<RecoveryEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const show = async () => {
    setOpen(true);
    setBusy(true);
    setError(null);
    try { setEntries(await getBackendTransport().listProjectRecovery()); }
    catch { setError('Recovery snapshots could not be loaded.'); }
    finally { setBusy(false); }
  };

  const restore = async (id: string) => {
    setBusy(true);
    setError(null);
    try {
      await useGraphStore.getState().restoreRecovery(id);
      setOpen(false);
    } catch { setError('Recovery failed. Your current graph is still open.'); }
    finally { setBusy(false); }
  };

  return <>
    <ToolbarButton icon={<HistoryIcon />} label="Recovery snapshots" onClick={() => void show()} />
    {open && <div className="stale-save-overlay">
      <div className="stale-save-dialog" role="dialog" aria-modal="true" aria-label="Recovery snapshots">
        <div className="stale-save-header"><h2>Recovery snapshots</h2></div>
        <div className="stale-save-content">
          <p>Open a completed snapshot as an unsaved Project, then choose where to save it. Your current unsaved edits are preserved first.</p>
          {busy && <p role="status">Working...</p>}
          {error && <p role="alert">{error}</p>}
          {!busy && entries.length === 0 && <p>No completed snapshots yet.</p>}
          <ul className="recovery-list">{entries.map((entry) => <li key={entry.id}>
            <span>{entry.sourcePath?.split(/[\\/]/).pop() ?? 'Untitled Project'}<small>{new Date(entry.createdEpochMs).toLocaleString()}</small></span>
            <button className="secondary" disabled={busy} onClick={() => void restore(entry.id)}>Open snapshot</button>
          </li>)}</ul>
        </div>
        <div className="stale-save-actions"><button className="secondary" disabled={busy} onClick={() => setOpen(false)}>Close</button></div>
      </div>
    </div>}
  </>;
}
