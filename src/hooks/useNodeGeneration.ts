import { useCallback } from 'react';
import { getBackendTransport } from '../lib/transport';
import { useGraphStore } from '../store/useGraphStore';
import { useProviderStore } from '../store/useProviderStore';
import { useUIStore } from '../store/useUIStore';
import type { AgentProvider } from '../types';
import { logger } from '../lib/logger';

interface GenerateNodeOptions {
  userNodeId: string;
  provider?: AgentProvider;
  modelId?: string;
  onAgentNodeCreated?: (agentNodeId: string) => void;
}

export function useNodeGeneration() {
  const nodeData = useGraphStore((state) => state.nodeData);
  const createAgentNodeDownstream = useGraphStore((state) => state.createAgentNodeDownstream);
  const buildConversationContext = useGraphStore((state) => state.buildConversationContext);
  const appendToNode = useGraphStore((state) => state.appendToNode);
  const stopStreaming = useGraphStore((state) => state.stopStreaming);
  const isNodeBlocked = useGraphStore((state) => state.isNodeBlocked);
  const sendBlocker = useGraphStore((state) => state.sendBlocker);
  const canGenerate = useGraphStore((state) => state.canGenerate);
  const getEffectiveEffort = useGraphStore((state) => state.getEffectiveEffort);
  const defaultProvider = useProviderStore((state) => state.defaultProvider);

  return useCallback(
    async ({ userNodeId, provider, modelId, onAgentNodeCreated }: GenerateNodeOptions): Promise<string | null> => {
      const data = nodeData.get(userNodeId);
      if (!data || data.role !== 'user') return null;

      if (isNodeBlocked(userNodeId)) return null;

      // A broken file node in the Lineage subgraph is refused here, before any ACP call.
      const blocker = sendBlocker(userNodeId);
      if (blocker) {
        useUIStore.getState().setNotice(blocker);
        return null;
      }

      // Text, inline images, or a file node in the lineage (file-only prompts get
      // the backend placeholder text).
      if (!canGenerate(userNodeId)) return null;

      const agentNodeId = createAgentNodeDownstream(userNodeId, provider, modelId);
      onAgentNodeCreated?.(agentNodeId);

      const context = buildConversationContext(userNodeId);
      const effort = getEffectiveEffort(provider ?? defaultProvider);
      const transport = getBackendTransport();

      try {
        await transport.sendPrompt({
          nodeId: agentNodeId,
          messages: context,
          provider,
          modelId,
          effort,
        });
      } catch (error) {
        logger.error('Generation failed:', error);
        appendToNode(agentNodeId, `\n\n[Error: ${String(error)}]`);
      } finally {
        stopStreaming(agentNodeId);
      }

      return agentNodeId;
    },
    [
      appendToNode,
      buildConversationContext,
      canGenerate,
      createAgentNodeDownstream,
      defaultProvider,
      getEffectiveEffort,
      isNodeBlocked,
      nodeData,
      sendBlocker,
      stopStreaming,
    ]
  );
}
