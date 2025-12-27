import { useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useGraphStore } from '../store/useGraphStore';

const SUMMARY_THRESHOLD = 100; // Characters - content shorter than this uses content directly
const DEBOUNCE_MS = 1500;      // Wait for content to stabilize before generating

interface SummaryResult {
  node_id: string;
  summary: string;
}

// Global queue for serializing summary generation (only one ACP subprocess at a time)
const summaryQueue: Array<{ nodeId: string; content: string; resolve: (result: SummaryResult) => void; reject: (error: unknown) => void }> = [];
let isProcessingQueue = false;

async function processQueue() {
  if (isProcessingQueue || summaryQueue.length === 0) return;

  isProcessingQueue = true;

  while (summaryQueue.length > 0) {
    const item = summaryQueue.shift()!;
    try {
      const result = await invoke<SummaryResult>('generate_summary', {
        nodeId: item.nodeId,
        content: item.content,
      });
      item.resolve(result);
    } catch (error) {
      item.reject(error);
    }
    // Small delay between calls to let subprocess clean up
    await new Promise(resolve => setTimeout(resolve, 500));
  }

  isProcessingQueue = false;
}

function queueSummaryGeneration(nodeId: string, content: string): Promise<SummaryResult> {
  return new Promise((resolve, reject) => {
    summaryQueue.push({ nodeId, content, resolve, reject });
    processQueue();
  });
}

/**
 * Hook that automatically generates summaries for node content.
 * - Short content (<=100 chars): uses content directly as summary
 * - Long content (>100 chars): calls Haiku via ACP to generate a heading
 */
export function useSummaryGeneration() {
  const pendingRef = useRef<Set<string>>(new Set());
  const timeoutsRef = useRef<Map<string, NodeJS.Timeout>>(new Map());

  const nodeData = useGraphStore((state) => state.nodeData);
  const streamingNodeIds = useGraphStore((state) => state.streamingNodeIds);
  const setSummary = useGraphStore((state) => state.setSummary);

  useEffect(() => {
    // Check all nodes for pending summaries
    for (const [nodeId, data] of nodeData) {
      // Skip if currently streaming
      if (streamingNodeIds.has(nodeId)) continue;

      // Skip if no content
      if (!data.content || !data.content.trim()) continue;

      // Short content: use directly as summary
      if (data.content.length <= SUMMARY_THRESHOLD) {
        // Only update if summary doesn't match content
        if (data.summary !== data.content) {
          setSummary(nodeId, data.content);
        }
        continue;
      }

      // Long content: need AI summary

      // Skip if already has valid summary for this content
      // (summaryTimestamp should be >= when content was last changed)
      if (data.summary && data.summaryTimestamp && data.summaryTimestamp >= data.timestamp) {
        continue;
      }

      // Skip if already pending
      if (pendingRef.current.has(nodeId)) continue;

      // Debounce to avoid regenerating during rapid edits
      const existingTimeout = timeoutsRef.current.get(nodeId);
      if (existingTimeout) {
        clearTimeout(existingTimeout);
      }

      const timeout = setTimeout(async () => {
        timeoutsRef.current.delete(nodeId);

        // Double-check it's not streaming now
        if (useGraphStore.getState().streamingNodeIds.has(nodeId)) return;

        // Check content hasn't changed significantly
        const currentData = useGraphStore.getState().nodeData.get(nodeId);
        if (!currentData || currentData.content.length <= SUMMARY_THRESHOLD) return;

        pendingRef.current.add(nodeId);

        try {
          console.log(`[Summary] Queueing summary generation for node ${nodeId}`);
          const result = await queueSummaryGeneration(nodeId, currentData.content);

          // Verify node still exists and needs summary
          const finalData = useGraphStore.getState().nodeData.get(nodeId);
          if (finalData && finalData.content.length > SUMMARY_THRESHOLD) {
            setSummary(result.node_id, result.summary);
            console.log(`[Summary] Set summary for ${nodeId}: ${result.summary}`);
          }
        } catch (error) {
          console.error(`[Summary] Failed to generate summary for ${nodeId}:`, error);
          // Use truncated content as fallback
          const fallback = currentData.content.slice(0, 50) + '...';
          setSummary(nodeId, fallback);
        } finally {
          pendingRef.current.delete(nodeId);
        }
      }, DEBOUNCE_MS);

      timeoutsRef.current.set(nodeId, timeout);
    }

    // Cleanup function
    return () => {
      for (const timeout of timeoutsRef.current.values()) {
        clearTimeout(timeout);
      }
    };
  }, [nodeData, streamingNodeIds, setSummary]);
}
