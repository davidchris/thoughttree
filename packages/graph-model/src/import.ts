import type { Graph, GraphEdge, GraphNode, NodeId, Position, TurnProvenance } from './types';

export interface ImportedConversationTurn {
  userMessage: string;
  assistantAnswer: string;
  incomplete?: boolean;
  model?: string;
  userTimestamp?: number;
  assistantTimestamp?: number;
  provenance?: TurnProvenance;
}

export interface ImportedConversation {
  importKey: string;
  turns: readonly ImportedConversationTurn[];
}

export interface TurnRange {
  startIndex?: number;
  endIndex?: number;
}

const NODE_VERTICAL_GAP = 120;

function nodeId(importKey: string, turnIndex: number, role: 'user' | 'assistant'): NodeId {
  return `import:${encodeURIComponent(importKey)}:turn:${turnIndex}:${role}`;
}

function cloneProvenance(provenance: TurnProvenance): TurnProvenance {
  return {
    completeness: provenance.completeness,
    references: provenance.references.map((reference) => ({
      ...reference,
      relations: [...reference.relations],
    })),
    activity: provenance.activity.map((activity) => ({ ...activity })),
  };
}

/**
 * Builds a new linear Graph from normalized conversation data. Turn ranges are
 * inclusive; merging or updating an existing Graph is intentionally out of scope.
 */
export function conversationToGraph(conversation: ImportedConversation, range: TurnRange = {}): Graph {
  const nodes = new Map<NodeId, GraphNode>();
  const edges: GraphEdge[] = [];
  const layout = new Map<NodeId, Position>();
  const startIndex = range.startIndex ?? 0;
  const endIndex = range.endIndex ?? conversation.turns.length - 1;
  let previousAssistantId: NodeId | undefined;

  for (const [turnIndex, turn] of conversation.turns.entries()) {
    if (turnIndex < startIndex || turnIndex > endIndex) continue;

    const userId = nodeId(conversation.importKey, turnIndex, 'user');
    const assistantId = nodeId(conversation.importKey, turnIndex, 'assistant');
    const userTimestamp = turn.userTimestamp ?? turnIndex * 2;
    const assistantTimestamp = turn.assistantTimestamp ?? turnIndex * 2 + 1;

    nodes.set(userId, {
      id: userId,
      role: 'user',
      content: turn.userMessage,
      timestamp: userTimestamp,
    });
    nodes.set(assistantId, {
      id: assistantId,
      role: 'assistant',
      content: turn.assistantAnswer,
      timestamp: assistantTimestamp,
      ...(turn.model === undefined ? {} : { model: turn.model }),
      ...(turn.incomplete ? { incomplete: true } : {}),
      ...(turn.provenance === undefined ? {} : { provenance: cloneProvenance(turn.provenance) }),
    });

    layout.set(userId, { x: 0, y: turnIndex * NODE_VERTICAL_GAP * 2 });
    layout.set(assistantId, { x: 0, y: (turnIndex * 2 + 1) * NODE_VERTICAL_GAP });

    if (previousAssistantId) {
      edges.push({
        id: `${previousAssistantId}->${userId}`,
        source: previousAssistantId,
        target: userId,
      });
    }
    edges.push({ id: `${userId}->${assistantId}`, source: userId, target: assistantId });
    previousAssistantId = assistantId;
  }

  return { nodes, edges, layout };
}
