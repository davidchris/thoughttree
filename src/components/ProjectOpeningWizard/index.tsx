import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './ProjectOpeningWizard.css';

interface ProjectOpeningWizardProps {
  onProjectSelected: (path: string) => void;
  onOpenDialog: () => void;
  onNewProject: () => void;
}

export function ProjectOpeningWizard({
  onProjectSelected,
  onOpenDialog,
  onNewProject,
}: ProjectOpeningWizardProps) {
  const [recentProjects, setRecentProjects] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const loadRecentProjects = async () => {
      try {
        const projects = await invoke<string[]>('get_recent_projects');
        // Filter out projects that no longer exist
        const validProjects = await Promise.all(
          projects.map(async (path) => {
            try {
              // Try to read the file to verify it exists
              await invoke('load_project', { path });
              return path;
            } catch {
              // File doesn't exist, remove it from recent projects
              try {
                await invoke('remove_recent_project', { path });
              } catch {
                // Ignore errors when removing
              }
              return null;
            }
          })
        );
        setRecentProjects(validProjects.filter((p): p is string => p !== null));
      } catch (e) {
        setError(`Failed to load recent projects: ${e}`);
      } finally {
        setIsLoading(false);
      }
    };

    loadRecentProjects();
  }, []);

  const handleProjectClick = async (path: string) => {
    try {
      await invoke('add_recent_project', { path });
      onProjectSelected(path);
    } catch (e) {
      console.error('Failed to update recent projects:', e);
      // Still try to open the project
      onProjectSelected(path);
    }
  };

  const handleRemoveProject = async (e: React.MouseEvent, path: string) => {
    e.stopPropagation();
    try {
      await invoke('remove_recent_project', { path });
      setRecentProjects((prev) => prev.filter((p) => p !== path));
    } catch (e) {
      console.error('Failed to remove project from recent:', e);
    }
  };

  const getProjectName = (path: string): string => {
    const fileName = path.split('/').pop() || path;
    return fileName.replace('.thoughttree', '') || 'Untitled';
  };

  const getProjectPath = (path: string): string => {
    const parts = path.split('/');
    if (parts.length > 2) {
      return `.../${parts.slice(-2).join('/')}`;
    }
    return path;
  };

  return (
    <div className="project-opening-wizard-overlay">
      <div className="project-opening-wizard">
        <h1>Open Project</h1>
        <p className="wizard-description">
          Select a recently opened project or open a new one to get started.
        </p>

        {isLoading ? (
          <div className="loading-state">
            <p>Loading recent projects...</p>
          </div>
        ) : error ? (
          <div className="error-state">
            <p className="error-message">{error}</p>
          </div>
        ) : (
          <>
            {recentProjects.length > 0 && (
              <div className="recent-projects-section">
                <h2>Recent Projects</h2>
                <div className="recent-projects-list">
                  {recentProjects.map((path) => (
                    <div
                      key={path}
                      className="recent-project-item"
                      onClick={() => handleProjectClick(path)}
                    >
                      <div className="project-info">
                        <div className="project-name">{getProjectName(path)}</div>
                        <div className="project-path">{getProjectPath(path)}</div>
                      </div>
                      <button
                        className="remove-project-button"
                        onClick={(e) => handleRemoveProject(e, path)}
                        title="Remove from recent"
                      >
                        ×
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            )}

            <div className="wizard-actions">
              <button onClick={onOpenDialog} className="action-button secondary">
                Open Project...
              </button>
              <button onClick={onNewProject} className="action-button primary">
                New Project
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

