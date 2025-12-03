import { useGraphStore } from '../../store/useGraphStore';
import { respondToPermission } from '../../lib/tauri';
import './styles.css';

export function PermissionDialog() {
  const { pendingPermission, setPendingPermission } = useGraphStore();

  if (!pendingPermission) {
    return null;
  }

  const handleOptionClick = async (optionId: string) => {
    try {
      await respondToPermission(pendingPermission.id, optionId);
    } catch (error) {
      console.error('Failed to respond to permission:', error);
    } finally {
      setPendingPermission(null);
    }
  };

  return (
    <div className="permission-overlay">
      <div className="permission-dialog">
        <div className="permission-header">
          <h2>Permission Request</h2>
          <span className="tool-type">{pendingPermission.toolType}</span>
        </div>

        <div className="permission-content">
          <p className="tool-name">{pendingPermission.toolName}</p>
          <p className="tool-description">{pendingPermission.description}</p>
        </div>

        <div className="permission-actions">
          {pendingPermission.options.map((option) => (
            <button
              key={option.id}
              onClick={() => handleOptionClick(option.id)}
              className={option.label.toLowerCase().includes('deny') ? 'deny' : 'approve'}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
