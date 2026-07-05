import { useEffect, useState } from 'react';
import { useGraphStore } from '../../store/useGraphStore';
import { useProviderStore } from '../../store/useProviderStore';
import { useUIStore } from '../../store/useUIStore';
import { MarkdownContent } from '../Graph/MarkdownContent';
import { PROVIDER_SHORT_NAMES, type AgentProvider, type AgentNodeData, type UserNodeData } from '../../types';
import { useNodeGeneration } from '../../hooks/useNodeGeneration';
import { logger } from '../../lib/logger';
import { usePanelResize } from './usePanelResize';
import { EditArea } from './EditArea';
import { GenerationControls } from './GenerationControls';
import './styles.css';

export function SidePanel() {
  const previewNodeId = useUIStore((state) => state.previewNodeId);
  const data = useGraphStore((state) =>
    previewNodeId ? state.nodeData.get(previewNodeId) : null
  );
  const setPreviewNode = useUIStore((state) => state.setPreviewNode);
  const streamingNodeIds = useGraphStore((state) => state.streamingNodeIds);
  const isNodeBlockedFn = useGraphStore((state) => state.isNodeBlocked);
  const defaultProvider = useProviderStore((state) => state.defaultProvider);
  const getEffectiveModel = useGraphStore((state) => state.getEffectiveModel);
  const triggerSidePanelEdit = useUIStore((state) => state.triggerSidePanelEdit);
  const clearSidePanelEditTrigger = useUIStore((state) => state.clearSidePanelEditTrigger);

  const [isEditing, setIsEditing] = useState(false);
  const [copySuccess, setCopySuccess] = useState(false);
  const [selectedProvider, setSelectedProvider] = useState<AgentProvider>(defaultProvider);
  const [selectedModel, setSelectedModel] = useState<string | undefined>(undefined);
  const { width, handleResizeStart } = usePanelResize();
  const generateNode = useNodeGeneration();

  const isUserNode = data?.role === 'user';
  const images = isUserNode ? (data as UserNodeData).images || [] : [];
  const isStreaming = previewNodeId ? streamingNodeIds.has(previewNodeId) : false;
  const isBlocked = previewNodeId ? isNodeBlockedFn(previewNodeId) : false;

  // Initialize selectedModel from effective model when user node is selected
  useEffect(() => {
    if (isUserNode) {
      const effectiveModel = getEffectiveModel(selectedProvider);
      setSelectedModel(effectiveModel);
    }
  }, [isUserNode, selectedProvider, getEffectiveModel]);

  // Reset edit state when node changes
  useEffect(() => {
    setIsEditing(false);
  }, [previewNodeId]);

  // React to keyboard shortcut trigger for edit mode
  useEffect(() => {
    if (triggerSidePanelEdit && isUserNode && data) {
      setIsEditing(true);
      clearSidePanelEditTrigger();
    }
  }, [triggerSidePanelEdit, isUserNode, data, clearSidePanelEditTrigger]);

  // Escape exits edit mode, then closes the panel. EditArea stops propagation
  // when Escape closes its autocomplete instead.
  useEffect(() => {
    if (!previewNodeId) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (isEditing) {
          setIsEditing(false);
        } else {
          setPreviewNode(null);
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [previewNodeId, setPreviewNode, isEditing]);

  const handleCopy = async () => {
    if (!data?.content) return;

    try {
      await navigator.clipboard.writeText(data.content);
      setCopySuccess(true);
      setTimeout(() => setCopySuccess(false), 2000);
    } catch (error) {
      logger.error('Failed to copy content:', error);
      // Fallback for older browsers
      const textarea = document.createElement('textarea');
      textarea.value = data.content;
      textarea.style.position = 'fixed';
      textarea.style.opacity = '0';
      document.body.appendChild(textarea);
      textarea.select();
      try {
        document.execCommand('copy');
        setCopySuccess(true);
        setTimeout(() => setCopySuccess(false), 2000);
      } catch (err) {
        logger.error('Fallback copy failed:', err);
      }
      document.body.removeChild(textarea);
    }
  };

  const handleGenerate = async () => {
    if (!previewNodeId || !data?.content.trim() || isBlocked) return;

    // Exit edit mode
    setIsEditing(false);

    // Use selected model or provider default.
    const modelToUse = selectedModel || undefined;
    await generateNode({
      userNodeId: previewNodeId,
      provider: selectedProvider,
      modelId: modelToUse,
      onAgentNodeCreated: (agentNodeId) => setPreviewNode(agentNodeId),
    });
  };

  if (!previewNodeId || !data) {
    return null;
  }

  const isAgent = data.role === 'assistant';
  const formattedTime = new Date(data.timestamp).toLocaleString();

  return (
    <div className="side-panel" style={{ width }}>
      <div
        className="side-panel-resize-handle"
        onMouseDown={handleResizeStart}
      />
      <div className="side-panel-header">
        <div className="side-panel-title">
          <span className={`side-panel-badge ${isAgent ? 'agent' : 'user'}`}>
            {isAgent
              ? ((data as AgentNodeData).provider
                  ? PROVIDER_SHORT_NAMES[(data as AgentNodeData).provider!]
                  : 'Assistant')
              : 'User'}
          </span>
          {isStreaming && <span className="side-panel-streaming">Generating...</span>}
          <span className="side-panel-timestamp">{formattedTime}</span>
        </div>
        <div className="side-panel-actions">
          {!isEditing && data?.content && (
            <button
              className="side-panel-copy-button"
              onClick={handleCopy}
              title="Copy as markdown"
            >
              {copySuccess ? 'Copied!' : 'Copy'}
            </button>
          )}
          {isUserNode && !isEditing && (
            <button
              className="side-panel-edit-button"
              onClick={() => setIsEditing(true)}
              title="Edit content"
            >
              Edit
            </button>
          )}
          {isUserNode && (
            <GenerationControls
              provider={selectedProvider}
              model={selectedModel}
              onProviderChange={setSelectedProvider}
              onModelChange={setSelectedModel}
              disabled={isBlocked}
              generateDisabled={isBlocked || !data?.content.trim()}
              onGenerate={handleGenerate}
            />
          )}
          {isEditing && (
            <button
              className="side-panel-done-button"
              onClick={() => setIsEditing(false)}
            >
              Done
            </button>
          )}
          <button
            className="side-panel-close"
            onClick={() => setPreviewNode(null)}
            title="Close (Escape)"
          >
            ×
          </button>
        </div>
      </div>
      <div className="side-panel-content">
        {isEditing ? (
          <EditArea
            nodeId={previewNodeId}
            initialContent={data.content}
            images={images}
            onGenerate={handleGenerate}
          />
        ) : data?.content ? (
          isStreaming ? (
            <pre className="side-panel-plain-text">{data.content}</pre>
          ) : (
            <MarkdownContent content={data.content} />
          )
        ) : isStreaming ? (
          <span className="side-panel-empty">Waiting for response...</span>
        ) : (
          <span className="side-panel-empty">No content</span>
        )}
      </div>
    </div>
  );
}
