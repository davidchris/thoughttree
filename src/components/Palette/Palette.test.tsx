import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { GraphNode } from '@thoughttree/graph-model';
import { Palette } from './index';
import { useGraphStore } from '../../store/useGraphStore';
import { useUIStore } from '../../store/useUIStore';

const mockSetCenter = vi.fn();
const mockGetNode = vi.fn();

vi.mock('@xyflow/react', () => ({
  useReactFlow: () => ({ setCenter: mockSetCenter, getNode: mockGetNode }),
}));

function userNode(id: string, content: string, overrides: Partial<GraphNode> = {}): GraphNode {
  return { id, role: 'user', content, timestamp: 0, ...overrides } as GraphNode;
}

function agentNode(id: string, content: string, overrides: Partial<GraphNode> = {}): GraphNode {
  return { id, role: 'assistant', content, timestamp: 0, ...overrides } as GraphNode;
}

function seedGraph(nodes: GraphNode[]) {
  useGraphStore.setState({
    graph: {
      nodes: new Map(nodes.map((n) => [n.id, n])),
      edges: [],
      layout: new Map(nodes.map((n) => [n.id, { x: 0, y: 0 }])),
    },
    selectedNodeId: null,
  });
}

function openPalette() {
  fireEvent.keyDown(window, { key: 'k', metaKey: true });
}

describe('Palette', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useUIStore.getState().reset();
    seedGraph([]);
  });

  it('opens with Cmd+K and shows nodes matching the typed query', async () => {
    seedGraph([
      userNode('a', 'the borrow checker fights the visitor'),
      agentNode('b', 'tokenizer design questions'),
    ]);
    render(<Palette />);

    expect(screen.queryByRole('dialog')).toBeNull();
    openPalette();
    const input = await screen.findByPlaceholderText(/search nodes/i);
    await userEvent.type(input, 'borrow');

    const options = screen.getAllByRole('option');
    expect(options).toHaveLength(1);
    expect(options[0].textContent).toContain('borrow checker');
  });

  it('closes on Escape without letting the event reach other window listeners', () => {
    seedGraph([userNode('a', 'hello')]);
    render(<Palette />);
    openPalette();
    // Simulates SidePanel/SettingsDialog-style bubble-phase Escape handlers.
    const outsideEscapeListener = vi.fn();
    window.addEventListener('keydown', outsideEscapeListener);
    // Escape originates from the focused palette input, like in the real app.
    fireEvent.keyDown(screen.getByPlaceholderText(/search nodes/i), { key: 'Escape' });

    expect(screen.queryByRole('dialog')).toBeNull();
    expect(outsideEscapeListener).not.toHaveBeenCalled();
    window.removeEventListener('keydown', outsideEscapeListener);
  });

  it('jumps to the top hit on Enter: selects, centers viewport, flashes, closes', async () => {
    seedGraph([
      userNode('a', 'the borrow checker fights'),
      agentNode('b', 'something else'),
    ]);
    mockGetNode.mockReturnValue({
      id: 'a',
      position: { x: 100, y: 50 },
      measured: { width: 200, height: 80 },
    });
    render(<Palette />);

    openPalette();
    const input = screen.getByPlaceholderText(/search nodes/i);
    await userEvent.type(input, 'borrow');
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(useGraphStore.getState().selectedNodeId).toBe('a');
    // Centered on the node's midpoint: position + measured/2.
    expect(mockSetCenter).toHaveBeenCalledWith(200, 90, { zoom: 1, duration: 400 });
    expect(useUIStore.getState().flashNodeId).toBe('a');
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('moves the active row with arrow keys before jumping', async () => {
    seedGraph([
      userNode('first', 'parser question one', { timestamp: 200 }),
      userNode('second', 'parser question two', { timestamp: 100 }),
    ]);
    render(<Palette />);

    openPalette();
    const input = screen.getByPlaceholderText(/search nodes/i);
    await userEvent.type(input, 'parser');
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(useGraphStore.getState().selectedNodeId).toBe('second');
  });

  it('additionally opens the side panel preview on Cmd+Enter', async () => {
    seedGraph([userNode('a', 'parser question')]);
    render(<Palette />);

    openPalette();
    const input = screen.getByPlaceholderText(/search nodes/i);
    await userEvent.type(input, 'parser');
    fireEvent.keyDown(input, { key: 'Enter', metaKey: true });

    expect(useGraphStore.getState().selectedNodeId).toBe('a');
    expect(useUIStore.getState().previewNodeId).toBe('a');
  });

  it('does not open while a modal flow (settings, permission prompt) is active', () => {
    seedGraph([userNode('a', 'hello')]);
    render(<Palette />);

    act(() => {
      useUIStore.setState({ settingsOpen: true });
    });
    openPalette();
    expect(screen.queryByRole('dialog')).toBeNull();

    act(() => {
      useUIStore.setState({
        settingsOpen: false,
        pendingPermission: {
          id: 'p1',
          toolType: 'fetch',
          toolName: 'WebFetch',
          description: '',
          options: [],
        },
      });
    });
    openPalette();
    expect(screen.queryByRole('dialog')).toBeNull();

    act(() => {
      useUIStore.setState({
        pendingPermission: null,
        staleProjectSave: { path: '/tmp/project.thoughttree', currentRevision: 'rev-2' },
      });
    });
    openPalette();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('closes when a permission request arrives while open', () => {
    seedGraph([userNode('a', 'hello')]);
    render(<Palette />);

    openPalette();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    act(() => {
      useUIStore.getState().setPendingPermission({
        id: 'p1',
        toolType: 'fetch',
        toolName: 'WebFetch',
        description: '',
        options: [],
      });
    });

    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('ignores Enter fired as part of an IME composition', async () => {
    seedGraph([userNode('a', 'parser question')]);
    render(<Palette />);

    openPalette();
    const input = screen.getByPlaceholderText(/search nodes/i);
    await userEvent.type(input, 'parser');
    fireEvent.keyDown(input, { key: 'Enter', isComposing: true });

    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(useGraphStore.getState().selectedNodeId).toBeNull();
  });

  it('traps Tab so focus cannot reach background controls', () => {
    seedGraph([userNode('a', 'hello')]);
    render(<Palette />);

    openPalette();
    const notCancelled = fireEvent.keyDown(screen.getByPlaceholderText(/search nodes/i), {
      key: 'Tab',
    });

    expect(notCancelled).toBe(false); // default prevented = focus stays inside
  });

  it('clears a stale editing state when opening', () => {
    seedGraph([userNode('a', 'hello')]);
    useUIStore.setState({ editingNodeId: 'a' });
    render(<Palette />);

    openPalette();

    expect(useUIStore.getState().editingNodeId).toBeNull();
  });
});
