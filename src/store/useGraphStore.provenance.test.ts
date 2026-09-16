import { beforeEach, describe, expect, it } from 'vitest';
import { GraphSerialize } from '@thoughttree/graph-model';
import type { AgentNodeData } from '../types';
import { setBackendTransport } from '../lib/transport';
import { createMockTransport } from '../test/mockTransport';
import { useGraphStore } from './useGraphStore';
import { useUIStore } from './useUIStore';

function resetStore() {
  useGraphStore.getState().newProject();
  useGraphStore.setState({ selectedNodeId: null, activeTurns: new Map<string, string>(), isDirty: false });
  useUIStore.getState().reset();
}

function agentNode() {
  const state = useGraphStore.getState();
  const userId = state.createUserNode();
  return state.createAgentNodeDownstream(userId, 'claude-code');
}

function provenanceOf(nodeId: string) {
  return (useGraphStore.getState().nodeData.get(nodeId) as AgentNodeData).provenance;
}

function turnOf(nodeId: string) {
  return useGraphStore.getState().activeTurns.get(nodeId)!;
}

const rawProvenance = {
  completeness: 'complete',
  references: [
    { type: 'file', scope: 'vault', path: 'notes.md', displayName: 'notes.md', relations: ['read'], timestamp: 1 },
    { type: 'file', scope: 'external', displayName: 'secret.txt', relations: ['read'], timestamp: 2 },
  ],
  activity: [
    { type: 'tool', kind: 'read', title: 'Read notes.md', status: 'completed', timestamp: 1, completedAt: 2 },
  ],
};

describe('useGraphStore Turn provenance capture', () => {
  beforeEach(() => {
    setBackendTransport(createMockTransport());
    resetStore();
  });

  it('attaches normalized provenance to the assistant node and marks the project dirty', () => {
    const agentId = agentNode();
    useGraphStore.setState({ isDirty: false });

    useGraphStore.getState().setTurnProvenance(agentId, turnOf(agentId), rawProvenance);

    expect(provenanceOf(agentId)).toEqual({
      completeness: 'complete',
      references: [
        { type: 'file', scope: 'vault', path: 'notes.md', displayName: 'notes.md', relations: ['read'], timestamp: 1 },
        { type: 'file', scope: 'external', displayName: 'secret.txt', relations: ['read'], timestamp: 2 },
      ],
      activity: [
        { type: 'tool', kind: 'read', title: 'Read notes.md', status: 'completed', timestamp: 1, completedAt: 2 },
      ],
    });
    expect(useGraphStore.getState().isDirty).toBe(true);
  });

  it('downgrades a vault reference whose path escapes the Vault', () => {
    const agentId = agentNode();

    useGraphStore.getState().setTurnProvenance(agentId, turnOf(agentId), {
      completeness: 'complete',
      references: [
        { type: 'file', scope: 'vault', path: '../etc/passwd', displayName: 'passwd', relations: ['read'] },
      ],
      activity: [],
    });

    expect(provenanceOf(agentId)?.completeness).toBe('partial');
    expect(provenanceOf(agentId)?.references).toEqual([
      { type: 'file', scope: 'external', displayName: 'passwd', relations: ['read'] },
    ]);
  });

  it('ignores provenance for user nodes, unknown nodes, and malformed payloads', () => {
    const state = useGraphStore.getState();
    const userId = state.createUserNode();
    const agentId = state.createAgentNodeDownstream(userId, 'claude-code');
    useGraphStore.setState({ isDirty: false });

    state.setTurnProvenance(userId, 'not-a-turn', rawProvenance);
    state.setTurnProvenance('missing', 'not-a-turn', rawProvenance);
    state.setTurnProvenance(agentId, turnOf(agentId), 'not an object');

    expect(useGraphStore.getState().nodeData.get(userId)).not.toHaveProperty('provenance');
    expect(useGraphStore.getState().nodeData.get(agentId)).not.toHaveProperty('provenance');
    expect(useGraphStore.getState().isDirty).toBe(false);
  });

  it('ignores provenance from an old Turn', () => {
    const agentId = agentNode();

    useGraphStore.getState().setTurnProvenance(agentId, 'old-turn', rawProvenance);

    expect(provenanceOf(agentId)).toBeUndefined();
  });

  it('survives a save round-trip', () => {
    const agentId = agentNode();
    useGraphStore.getState().setTurnProvenance(agentId, turnOf(agentId), rawProvenance);

    const restored = GraphSerialize.fromJSON(GraphSerialize.toJSON(useGraphStore.getState().graph));

    const node = restored.nodes.get(agentId);
    expect(node?.role === 'assistant' ? node.provenance?.references : undefined).toHaveLength(2);
  });
});
