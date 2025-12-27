import { useEffect, useState, useRef, useCallback } from 'react';
import { useGraphStore } from '../../store/useGraphStore';
import { MarkdownContent } from '../Graph/MarkdownContent';
import { sendPrompt, getAvailableModels } from '../../lib/tauri';
import { ProviderSelector } from '../ProviderSelector';
import { ModelSelector } from '../ModelSelector';
import { PROVIDER_SHORT_NAMES, type AgentProvider, type AgentNodeData } from '../../types';
import './styles.css';

const DEFAULT_WIDTH = 850; // ~100 character columns at 14px monospace
const MIN_WIDTH = 200;
const MAX_WIDTH_PERCENT = 0.8; // 80% of viewport

export function SidePanel() {
  const previewNodeId = useGraphStore((state) => state.previewNodeId);
  const data = useGraphStore((state) =>
    state.previewNodeId ? state.nodeData.get(state.previewNodeId) : null
  );
  const setPreviewNode = useGraphStore((state) => state.setPreviewNode);
  const updateNodeContent = useGraphStore((state) => state.updateNodeContent);
  const streamingNodeId = useGraphStore((state) => state.streamingNodeId);
  const createAgentNodeDownstream = useGraphStore((state) => state.createAgentNodeDownstream);
  const buildConversationContext = useGraphStore((state) => state.buildConversationContext);
  const appendToNode = useGraphStore((state) => state.appendToNode);
  const setStreaming = useGraphStore((state) => state.setStreaming);
  const defaultProvider = useGraphStore((state) => state.defaultProvider);
  const availableProviders = useGraphStore((state) => state.availableProviders);
  const availableModels = useGraphStore((state) => state.availableModels);
  const setAvailableModels = useGraphStore((state) => state.setAvailableModels);
  const getEffectiveModel = useGraphStore((state) => state.getEffectiveModel);
  const triggerSidePanelEdit = useGraphStore((state) => state.triggerSidePanelEdit);
  const clearSidePanelEditTrigger = useGraphStore((state) => state.clearSidePanelEditTrigger);

  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const [isResizing, setIsResizing] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState('');
  const [copySuccess, setCopySuccess] = useState(false);
  const [selectedProvider, setSelectedProvider] = useState<AgentProvider>(defaultProvider);
  const [selectedModel, setSelectedModel] = useState<string | undefined>(undefined);
  const [loadingModels, setLoadingModels] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const isUserNode = data?.role === 'user';
  const isStreaming = previewNodeId === streamingNodeId;
  const isAnyStreaming = streamingNodeId !== null;

  // Fetch models when provider changes (lazy load)
  const fetchModels = useCallback(async (provider: AgentProvider) => {
    // Skip if already loaded
    if (availableModels[provider]?.length > 0) return;

    setLoadingModels(true);
    try {
      const models = await getAvailableModels(provider);
      setAvailableModels(provider, models);
    } catch (error) {
      console.error('Failed to fetch models:', error);
    } finally {
      setLoadingModels(false);
    }
  }, [availableModels, setAvailableModels]);

  // Fetch models when user node is selected or provider changes
  useEffect(() => {
    if (isUserNode && selectedProvider) {
      fetchModels(selectedProvider);
    }
  }, [isUserNode, selectedProvider, fetchModels]);

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
    if (data) {
      setEditContent(data.content);
    }
  }, [previewNodeId]);

  // Focus textarea when entering edit mode
  useEffect(() => {
    if (isEditing && textareaRef.current) {
      textareaRef.current.focus();
      textareaRef.current.selectionStart = textareaRef.current.value.length;
    }
  }, [isEditing]);

  // React to keyboard shortcut trigger for edit mode
  useEffect(() => {
    if (triggerSidePanelEdit && isUserNode && data) {
      setEditContent(data.content);
      setIsEditing(true);
      clearSidePanelEditTrigger();
    }
  }, [triggerSidePanelEdit, isUserNode, data, clearSidePanelEditTrigger]);

  // Handle Escape key
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

  const handleEdit = () => {
    if (data) {
      setEditContent(data.content);
      setIsEditing(true);
    }
  };

  const handleDone = () => {
    setIsEditing(false);
  };

  const handleContentChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newValue = e.target.value;
    setEditContent(newValue);
    if (previewNodeId) {
      updateNodeContent(previewNodeId, newValue);
    }
  };

  const handleTextareaKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Cmd/Ctrl + Enter to generate
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      handleGenerate();
    }
  };

  const handleCopy = async () => {
    if (!data?.content) return;

    try {
      await navigator.clipboard.writeText(data.content);
      setCopySuccess(true);
      setTimeout(() => setCopySuccess(false), 2000);
    } catch (error) {
      console.error('Failed to copy content:', error);
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
        console.error('Fallback copy failed:', err);
      }
      document.body.removeChild(textarea);
    }
  };

  const handleGenerate = async () => {
    if (!previewNodeId || !data?.content.trim() || isAnyStreaming) return;

    // Exit edit mode
    setIsEditing(false);

    // Use selected model or fall back to empty string for default
    const modelToUse = selectedModel || undefined;

    // Create downstream agent node with selected provider and model
    const agentNodeId = createAgentNodeDownstream(previewNodeId, selectedProvider, modelToUse);

    // Switch preview to the new agent node to show streaming response
    setPreviewNode(agentNodeId);

    // Build context by traversing parents (including the edited user node)
    const context = buildConversationContext(previewNodeId);

    try {
      await sendPrompt(agentNodeId, context, (chunk) =>
        appendToNode(agentNodeId, chunk),
        selectedProvider,
        modelToUse
      );
    } catch (error) {
      console.error('Generation failed:', error);
      appendToNode(agentNodeId, `\n\n[Error: ${error}]`);
    } finally {
      setStreaming(null);
    }
  };

  // Handle resize drag
  useEffect(() => {
    if (!isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      const maxWidth = window.innerWidth * MAX_WIDTH_PERCENT;
      const newWidth = Math.max(MIN_WIDTH, Math.min(maxWidth, window.innerWidth - e.clientX));
      setWidth(newWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
      document.body.classList.remove('resizing');
    };

    document.body.classList.add('resizing');
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizing]);

  const handleResizeStart = (e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
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
              onClick={handleEdit}
              title="Edit content"
            >
              Edit
            </button>
          )}
          {isUserNode && (
            <>
              {availableProviders.length > 0 && (
                <ProviderSelector
                  value={selectedProvider}
                  onChange={(provider) => {
                    setSelectedProvider(provider);
                    // Reset model selection when provider changes
                    setSelectedModel(undefined);
                  }}
                  availableProviders={availableProviders}
                  disabled={isAnyStreaming}
                  compact
                />
              )}
              <ModelSelector
                provider={selectedProvider}
                value={selectedModel}
                onChange={setSelectedModel}
                availableModels={availableModels[selectedProvider] ?? []}
                disabled={isAnyStreaming}
                loading={loadingModels}
                compact
              />
              <button
                className="side-panel-generate-button"
                onClick={handleGenerate}
                disabled={isAnyStreaming || !data?.content.trim()}
                title="Generate response (Cmd+Enter)"
              >
                {isAnyStreaming ? 'Generating...' : 'Generate'}
              </button>
            </>
          )}
          {isEditing && (
            <button
              className="side-panel-done-button"
              onClick={handleDone}
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
          <textarea
            ref={textareaRef}
            className="side-panel-textarea"
            value={editContent}
            onChange={handleContentChange}
            onKeyDown={handleTextareaKeyDown}
            placeholder="Enter your message..."
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
