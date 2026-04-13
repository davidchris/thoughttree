import { useCallback } from 'react';
import { sendPrompt } from '../lib/tauri';
import { useGraphStore } from '../store/useGraphStore';
import type { AgentProvider, UserNodeData } from '../types';
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

  return useCallback(
    async ({ userNodeId, provider, modelId, onAgentNodeCreated }: GenerateNodeOptions): Promise<string | null> => {
      const data = nodeData.get(userNodeId);
      if (!data || data.role !== 'user') return null;

      const userData = data as UserNodeData;
      const hasContent = !!userData.content.trim();
      const hasImages = !!(userData.images && userData.images.length > 0);
      if (!hasContent && !hasImages) return null;

      if (isNodeBlocked(userNodeId)) return null;

      const agentNodeId = createAgentNodeDownstream(userNodeId, provider, modelId);
      onAgentNodeCreated?.(agentNodeId);

      const context = buildConversationContext(userNodeId);

      try {
        await sendPrompt(
          agentNodeId,
          context,
          (chunk) => appendToNode(agentNodeId, chunk),
          provider,
          modelId
        );
      } catch (error) {
        logger.error('Generation failed:', error);
        appendToNode(agentNodeId, `\n\n[Error: ${String(error)}]`);
      } finally {
        stopStreaming(agentNodeId);
      }

      return agentNodeId;
    },
    [appendToNode, buildConversationContext, createAgentNodeDownstream, isNodeBlocked, nodeData, stopStreaming]
  );
}
