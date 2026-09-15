import { useEffect, useState } from 'react';
import { useGraphStore } from '../../store/useGraphStore';
import { useProviderStore } from '../../store/useProviderStore';
import { useUIStore } from '../../store/useUIStore';
import { providerShortName, type AgentProvider, type AgentNodeData, type UserNodeData } from '../../types';
import { useNodeGeneration } from '../../hooks/useNodeGeneration';
import { usePanelResize } from './usePanelResize';
import { FilePanel } from './FilePanel';
import { SidePanelActions } from './SidePanelActions';
import { SidePanelBody } from './SidePanelBody';
import { fileTypeBadge } from '../../lib/fileNodes';
import './styles.css';

export function SidePanel() {
  const previewNodeId = useUIStore((state) => state.previewNodeId);
  const data = useGraphStore((state) =>
    previewNodeId ? state.nodeData.get(previewNodeId) : null
  );
  const setPreviewNode = useUIStore((state) => state.setPreviewNode);
  const activeTurns = useGraphStore((state) => state.activeTurns);
  const isNodeBlockedFn = useGraphStore((state) => state.isNodeBlocked);
  const defaultProvider = useProviderStore((state) => state.defaultProvider);
  const getEffectiveModel = useGraphStore((state) => state.getEffectiveModel);
  const triggerSidePanelEdit = useUIStore((state) => state.triggerSidePanelEdit);
  const clearSidePanelEditTrigger = useUIStore((state) => state.clearSidePanelEditTrigger);

  const [isEditing, setIsEditing] = useState(false);
  const [selectedProvider, setSelectedProvider] = useState<AgentProvider>(defaultProvider);
  const [selectedModel, setSelectedModel] = useState<string | undefined>(undefined);
  const { width, handleResizeStart } = usePanelResize();
  const generateNode = useNodeGeneration();

  const isUserNode = data?.role === 'user';
  const images = isUserNode ? (data as UserNodeData).images || [] : [];
  const isStreaming = previewNodeId ? activeTurns.has(previewNodeId) : false;
  const isBlocked = previewNodeId ? isNodeBlockedFn(previewNodeId) : false;
  // Why this user node must not be sent right now (broken file node in its Lineage subgraph).
  const sendBlockedReason = useGraphStore((state) =>
    isUserNode && previewNodeId ? state.sendBlocker(previewNodeId) : null
  );
  // Text, inline images, or a file node in the lineage, and no blocker.
  const canGenerate = useGraphStore((state) =>
    isUserNode && previewNodeId ? state.canGenerate(previewNodeId) : false
  );

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

  const handleGenerate = async () => {
    if (!previewNodeId || !canGenerate || isBlocked) return;

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
  const generate = () => void handleGenerate();

  if (!previewNodeId || !data) {
    return null;
  }

  const isAgent = data.role === 'assistant';
  const provenance = isAgent ? (data as AgentNodeData).provenance : undefined;
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
              ? providerShortName((data as AgentNodeData).provider)
              : isUserNode ? 'User' : fileTypeBadge(data.name)}
          </span>
          {isStreaming && <span className="side-panel-streaming">Generating...</span>}
          <span className="side-panel-timestamp">{formattedTime}</span>
        </div>
        <SidePanelActions
          content={data.content}
          isUserNode={isUserNode}
          isEditing={isEditing}
          isBlocked={isBlocked}
          canGenerate={canGenerate}
          generateBlockedReason={sendBlockedReason}
          provider={selectedProvider}
          model={selectedModel}
          onProviderChange={setSelectedProvider}
          onModelChange={setSelectedModel}
          onGenerate={generate}
          onStartEdit={() => setIsEditing(true)}
          onFinishEdit={() => setIsEditing(false)}
          onClose={() => setPreviewNode(null)}
        />
      </div>
      {data.role === 'file' ? (
        <div className="side-panel-content">
          <FilePanel key={previewNodeId} node={data} />
        </div>
      ) : (
        <SidePanelBody
          nodeId={previewNodeId}
          content={data.content}
          images={images}
          isEditing={isEditing}
          isStreaming={isStreaming}
          provenance={provenance}
          onGenerate={generate}
        />
      )}
    </div>
  );
}
