import { useEffect, useState, useRef, useCallback } from 'react';
import { useGraphStore } from '../../store/useGraphStore';
import { MarkdownContent } from '../Graph/MarkdownContent';
import { sendPrompt, getAvailableModels } from '../../lib/tauri';
import { ProviderSelector } from '../ProviderSelector';
import { ModelSelector } from '../ModelSelector';
import { FileAutocomplete, FileAutocompleteRef } from '../FileAutocomplete';
import { PROVIDER_SHORT_NAMES, type AgentProvider, type AgentNodeData, type UserNodeData, type ImageAttachment } from '../../types';
import { resizeIfNeeded, fileToBase64 } from '../../lib/imageUtils';
import './styles.css';

interface AutocompleteState {
  isOpen: boolean;
  query: string;
  position: { top: number; left: number };
  triggerIndex: number;
}

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
  const streamingNodeIds = useGraphStore((state) => state.streamingNodeIds);
  const createAgentNodeDownstream = useGraphStore((state) => state.createAgentNodeDownstream);
  const buildConversationContext = useGraphStore((state) => state.buildConversationContext);
  const appendToNode = useGraphStore((state) => state.appendToNode);
  const stopStreaming = useGraphStore((state) => state.stopStreaming);
  const isNodeBlockedFn = useGraphStore((state) => state.isNodeBlocked);
  const defaultProvider = useGraphStore((state) => state.defaultProvider);
  const availableProviders = useGraphStore((state) => state.availableProviders);
  const availableModels = useGraphStore((state) => state.availableModels);
  const setAvailableModels = useGraphStore((state) => state.setAvailableModels);
  const getEffectiveModel = useGraphStore((state) => state.getEffectiveModel);
  const triggerSidePanelEdit = useGraphStore((state) => state.triggerSidePanelEdit);
  const clearSidePanelEditTrigger = useGraphStore((state) => state.clearSidePanelEditTrigger);
  const addNodeImage = useGraphStore((state) => state.addNodeImage);
  const removeNodeImage = useGraphStore((state) => state.removeNodeImage);

  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const [isResizing, setIsResizing] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState('');
  const [copySuccess, setCopySuccess] = useState(false);
  const [selectedProvider, setSelectedProvider] = useState<AgentProvider>(defaultProvider);
  const [selectedModel, setSelectedModel] = useState<string | undefined>(undefined);
  const [loadingModels, setLoadingModels] = useState(false);
  const [autocomplete, setAutocomplete] = useState<AutocompleteState | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const autocompleteRef = useRef<FileAutocompleteRef>(null);

  const isUserNode = data?.role === 'user';
  const images = isUserNode ? (data as UserNodeData).images || [] : [];
  const isStreaming = previewNodeId ? streamingNodeIds.has(previewNodeId) : false;
  const isBlocked = previewNodeId ? isNodeBlockedFn(previewNodeId) : false;

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
    setAutocomplete(null);
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
        if (autocomplete?.isOpen) {
          setAutocomplete(null);
        } else if (isEditing) {
          setIsEditing(false);
        } else {
          setPreviewNode(null);
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [previewNodeId, setPreviewNode, isEditing, autocomplete]);

  const handleEdit = () => {
    if (data) {
      setEditContent(data.content);
      setIsEditing(true);
    }
  };

  const handleDone = () => {
    setIsEditing(false);
    setAutocomplete(null);
  };

  const handleContentChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newValue = e.target.value;
    const cursorPos = e.target.selectionStart;

    setEditContent(newValue);
    if (previewNodeId) {
      updateNodeContent(previewNodeId, newValue);
    }

    // Check for @ trigger
    const textBeforeCursor = newValue.slice(0, cursorPos);
    const atIndex = textBeforeCursor.lastIndexOf('@');

    if (atIndex !== -1) {
      // Check if @ is at start or preceded by whitespace
      const charBefore = atIndex > 0 ? textBeforeCursor[atIndex - 1] : ' ';
      if (/\s/.test(charBefore) || atIndex === 0) {
        const query = textBeforeCursor.slice(atIndex + 1);

        // Only show if query doesn't contain whitespace (still typing the mention)
        if (!/\s/.test(query)) {
          const textarea = e.target;
          const rect = textarea.getBoundingClientRect();
          const position = { top: rect.bottom + 4, left: rect.left };
          setAutocomplete({
            isOpen: true,
            query,
            position,
            triggerIndex: atIndex,
          });
          return;
        }
      }
    }

    // Close autocomplete if no valid trigger
    if (autocomplete?.isOpen) {
      setAutocomplete(null);
    }
  };

  const handleTextareaBlur = (e: React.FocusEvent) => {
    // Check if focus is moving to autocomplete
    const relatedTarget = e.relatedTarget as HTMLElement;
    if (relatedTarget?.closest('.file-autocomplete')) {
      return; // Don't close autocomplete
    }
    setAutocomplete(null);
  };

  const handleTextareaKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Let autocomplete handle keys first if open
    if (autocomplete?.isOpen && autocompleteRef.current) {
      const handled = autocompleteRef.current.handleKeyDown(e.nativeEvent);
      if (handled) {
        e.preventDefault();
        return;
      }
    }

    // Cmd/Ctrl + Enter to generate
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      handleGenerate();
    }
  };

  // Process and add an image file
  const processImageFile = useCallback(async (file: File) => {
    if (!file.type.startsWith('image/') || !previewNodeId) return;

    try {
      const resized = await resizeIfNeeded(file);
      const base64 = await fileToBase64(resized);
      const image: ImageAttachment = {
        data: base64,
        mimeType: file.type,
        name: file.name,
      };
      addNodeImage(previewNodeId, image);
    } catch (error) {
      console.error('Failed to process image:', error);
    }
  }, [previewNodeId, addNodeImage]);

  // Handle paste events (for Cmd+V with images)
  const handlePaste = useCallback(async (e: React.ClipboardEvent) => {
    const items = e.clipboardData?.items;
    if (!items) return;

    for (const item of Array.from(items)) {
      if (item.type.startsWith('image/')) {
        e.preventDefault();
        const file = item.getAsFile();
        if (file) {
          await processImageFile(file);
        }
      }
    }
  }, [processImageFile]);

  // Handle drag over
  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer.types.includes('Files')) {
      setIsDragOver(true);
    }
  }, []);

  // Handle drag leave - only if actually leaving the container
  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    // Check if we're leaving to a child element (don't reset in that case)
    const relatedTarget = e.relatedTarget as Node | null;
    if (relatedTarget && e.currentTarget.contains(relatedTarget)) {
      return;
    }
    setIsDragOver(false);
  }, []);

  // Handle drop events
  const handleDrop = useCallback(async (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);

    const files = e.dataTransfer.files;
    for (const file of Array.from(files)) {
      if (file.type.startsWith('image/')) {
        await processImageFile(file);
      }
    }
  }, [processImageFile]);

  // Handle image removal
  const handleRemoveImage = useCallback((index: number, e: React.MouseEvent) => {
    e.stopPropagation();
    if (previewNodeId) {
      removeNodeImage(previewNodeId, index);
    }
  }, [previewNodeId, removeNodeImage]);

  const handleFileSelect = (filePath: string) => {
    if (!autocomplete || !textareaRef.current || !previewNodeId) return;

    const textarea = textareaRef.current;
    const beforeAt = editContent.slice(0, autocomplete.triggerIndex);
    const afterCursor = editContent.slice(textarea.selectionStart);

    // Format: @/relative/path/to/file.md
    const mention = `@/${filePath}`;
    const newContent = beforeAt + mention + afterCursor;

    setEditContent(newContent);
    updateNodeContent(previewNodeId, newContent);
    setAutocomplete(null);

    // Position cursor after the mention
    const newCursorPos = autocomplete.triggerIndex + mention.length;
    setTimeout(() => {
      textarea.selectionStart = newCursorPos;
      textarea.selectionEnd = newCursorPos;
      textarea.focus();
    }, 0);
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
    if (!previewNodeId || !data?.content.trim() || isBlocked) return;

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
      stopStreaming(agentNodeId);
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
                  disabled={isBlocked}
                  compact
                />
              )}
              <ModelSelector
                provider={selectedProvider}
                value={selectedModel}
                onChange={setSelectedModel}
                availableModels={availableModels[selectedProvider] ?? []}
                disabled={isBlocked}
                loading={loadingModels}
                compact
              />
              <button
                className="side-panel-generate-button"
                onClick={handleGenerate}
                disabled={isBlocked || !data?.content.trim()}
                title="Generate response (Cmd+Enter)"
              >
                {isBlocked ? 'Generating...' : 'Generate'}
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
          <div
            className={`side-panel-edit-area ${isDragOver ? 'drag-over' : ''}`}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
          >
            <textarea
              ref={textareaRef}
              className="side-panel-textarea"
              value={editContent}
              onChange={handleContentChange}
              onBlur={handleTextareaBlur}
              onKeyDown={handleTextareaKeyDown}
              onPaste={handlePaste}
              onDragOver={handleDragOver}
              onDrop={handleDrop}
              placeholder="Enter your message... (@ to mention files, paste or drop images)"
            />
            {images.length > 0 && (
              <div className="side-panel-image-thumbnails">
                {images.map((img, index) => (
                  <div key={index} className="side-panel-image-thumbnail">
                    <img
                      src={`data:${img.mimeType};base64,${img.data}`}
                      alt={img.name || `Image ${index + 1}`}
                    />
                    <button
                      className="side-panel-image-remove"
                      onClick={(e) => handleRemoveImage(index, e)}
                      title="Remove image"
                    >
                      ×
                    </button>
                  </div>
                ))}
              </div>
            )}
            {autocomplete?.isOpen && (
              <FileAutocomplete
                ref={autocompleteRef}
                isOpen={autocomplete.isOpen}
                query={autocomplete.query}
                position={autocomplete.position}
                onSelect={handleFileSelect}
                onClose={() => setAutocomplete(null)}
              />
            )}
          </div>
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
