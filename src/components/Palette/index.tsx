import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useReactFlow } from '@xyflow/react';
import { useGraphStore } from '../../store/useGraphStore';
import { useUIStore } from '../../store/useUIStore';
import { PaletteSearch, type HighlightedText } from '../../lib/palette';
import type { GraphNode } from '../../lib/graph';
import { PROVIDER_SHORT_NAMES } from '../../types';
import './styles.css';

const MAX_VISIBLE_HITS = 20;
const JUMP_ZOOM = 1;
const JUMP_DURATION_MS = 400;
const DEFAULT_NODE_SIZE = 120;

function roleLabel(node: GraphNode): string {
  if (node.role === 'user') return 'User';
  return node.provider ? PROVIDER_SHORT_NAMES[node.provider] : 'Assistant';
}

function Highlighted({ value }: { value: HighlightedText }) {
  const parts: ReactNode[] = [];
  let cursor = 0;
  for (const span of value.spans) {
    if (span.start > cursor) {
      parts.push(value.text.slice(cursor, span.start));
    }
    parts.push(<mark key={span.start}>{value.text.slice(span.start, span.end)}</mark>);
    cursor = span.end;
  }
  if (cursor < value.text.length) {
    parts.push(value.text.slice(cursor));
  }
  return <>{parts}</>;
}

export function Palette() {
  // Non-null corpus doubles as the open flag: it is the Corpus snapshot
  // captured at open, kept stable so results never re-rank mid-navigation.
  const [corpus, setCorpus] = useState<readonly GraphNode[] | null>(null);
  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);
  const { setCenter, getNode } = useReactFlow();
  const isOpen = corpus !== null;

  const { hits: visibleHits, total } = useMemo(
    () =>
      corpus ? PaletteSearch.search(corpus, query, MAX_VISIBLE_HITS) : { hits: [], total: 0 },
    [corpus, query]
  );

  const open = useCallback(() => {
    // Modal flows own the keyboard; jumping mid-flow makes no sense.
    const { settingsOpen, pendingPermission, setEditing } = useUIStore.getState();
    if (settingsOpen || pendingPermission) return;
    // The palette steals focus from any in-edit textarea; an empty node's
    // blur handler won't clear editingNodeId, so reconcile it here or every
    // Graph shortcut stays dead behind `if (editingNodeId) return`.
    setEditing(null);
    const { graph } = useGraphStore.getState();
    setCorpus(Array.from(graph.nodes.values()));
    setQuery('');
    setActiveIndex(0);
  }, []);

  const close = useCallback(() => {
    setCorpus(null);
    setQuery('');
    setActiveIndex(0);
  }, []);

  const jump = useCallback(
    (nodeId: string, options: { preview?: boolean } = {}) => {
      const { graph, selectNode } = useGraphStore.getState();
      // Deleted while the palette was open: nothing to jump to.
      if (!graph.nodes.has(nodeId)) {
        close();
        return;
      }
      selectNode(nodeId);
      const flowNode = getNode(nodeId);
      if (flowNode) {
        const width = flowNode.measured?.width ?? DEFAULT_NODE_SIZE;
        const height = flowNode.measured?.height ?? DEFAULT_NODE_SIZE;
        setCenter(flowNode.position.x + width / 2, flowNode.position.y + height / 2, {
          zoom: JUMP_ZOOM,
          duration: JUMP_DURATION_MS,
        });
      }
      useUIStore.getState().flashNode(nodeId);
      if (options.preview) useUIStore.getState().setPreviewNode(nodeId);
      close();
    },
    [close, getNode, setCenter]
  );

  // A modal flow starting while the palette is open takes precedence: without
  // this, a PermissionDialog would render underneath the palette overlay with
  // its keyboard input swallowed.
  const pendingPermission = useUIStore((state) => state.pendingPermission);
  const settingsOpen = useUIStore((state) => state.settingsOpen);
  useEffect(() => {
    if (isOpen && (pendingPermission || settingsOpen)) close();
  }, [isOpen, pendingPermission, settingsOpen, close]);

  // Keep the active row visible under keyboard navigation.
  useEffect(() => {
    const active = listRef.current?.querySelector('[aria-selected="true"]');
    active?.scrollIntoView?.({ block: 'nearest' });
  }, [activeIndex, visibleHits]);

  // The handler reads volatile state via refs so it attaches once per mount
  // (same pattern as Graph's keyboard effect) — re-registering a capture-phase
  // listener per keystroke would make its ordering against other capture
  // listeners nondeterministic.
  const isOpenRef = useRef(isOpen);
  isOpenRef.current = isOpen;
  const visibleHitsRef = useRef(visibleHits);
  visibleHitsRef.current = visibleHits;
  const activeIndexRef = useRef(activeIndex);
  activeIndexRef.current = activeIndex;

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // An IME composition commit must never be treated as a shortcut.
      if (e.isComposing) return;
      // e.code covers non-Latin layouts where the physical K emits another key.
      if ((e.metaKey || e.ctrlKey) && (e.key.toLowerCase() === 'k' || e.code === 'KeyK')) {
        e.preventDefault();
        if (isOpenRef.current) close();
        else open();
        return;
      }
      if (!isOpenRef.current) return;
      if (e.key === 'Escape') {
        // Swallow before bubble-phase listeners (SidePanel, SettingsDialog) see it.
        e.preventDefault();
        e.stopPropagation();
        close();
      } else if (e.key === 'Enter') {
        e.preventDefault();
        e.stopPropagation();
        const hit = visibleHitsRef.current[activeIndexRef.current];
        if (hit) jump(hit.node.id, { preview: e.metaKey || e.ctrlKey });
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIndex((i) => Math.min(i + 1, visibleHitsRef.current.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === 'Tab') {
        // Focus trap: the input is the palette's only focusable control;
        // letting Tab reach background buttons would let Space/Enter activate
        // them behind the open overlay.
        e.preventDefault();
      }
    };
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [open, close, jump]);

  if (!isOpen) return null;

  return (
    <div
      className="palette-overlay"
      // mousedown, not click: a text-selection drag that starts in the input
      // and ends over the backdrop fires click on the overlay (common-ancestor
      // semantics) and would close the palette mid-selection.
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <div className="palette" role="dialog" aria-modal="true" aria-label="Node search">
        <input
          className="palette-input"
          placeholder="Search nodes…"
          autoFocus
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActiveIndex(0);
          }}
        />
        <ul className="palette-results" role="listbox" ref={listRef}>
          {visibleHits.map((hit, index) => (
            <li
              key={hit.node.id}
              role="option"
              aria-selected={index === activeIndex}
              className={`palette-result ${index === activeIndex ? 'active' : ''}`}
              onMouseEnter={() => setActiveIndex(index)}
              onClick={(e) => jump(hit.node.id, { preview: e.metaKey || e.ctrlKey })}
            >
              <div className="palette-result-main">
                <div className="palette-result-title">
                  <Highlighted value={hit.title} />
                </div>
                {hit.snippet && (
                  <div className="palette-result-snippet">
                    <Highlighted value={hit.snippet} />
                  </div>
                )}
              </div>
              <span className={`palette-result-role ${hit.node.role}`}>
                {roleLabel(hit.node)}
              </span>
            </li>
          ))}
          {visibleHits.length === 0 && (
            <li className="palette-empty">
              {query ? 'No matching nodes' : 'No nodes yet'}
            </li>
          )}
        </ul>
        <div className="palette-footer">
          {total > MAX_VISIBLE_HITS && <span className="palette-count">{total} matches</span>}
          <span className="palette-hints">
            <kbd>↑↓</kbd> navigate <kbd>↵</kbd> jump <kbd>⌘↵</kbd> open panel{' '}
            <kbd>esc</kbd> close
          </span>
        </div>
      </div>
    </div>
  );
}
