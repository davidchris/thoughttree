import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SidePanel } from "./index";
import { useGraphStore } from "../../store/useGraphStore";

// Mock the store
vi.mock("../../store/useGraphStore");

// Mock tauri lib
vi.mock("../../lib/tauri", () => ({
  sendPrompt: vi.fn(() => Promise.resolve()),
}));

const mockUseGraphStore = vi.mocked(useGraphStore);

describe("SidePanel", () => {
  const mockSetPreviewNode = vi.fn();
  const mockUpdateNodeContent = vi.fn();
  const mockCreateAgentNodeDownstream = vi.fn(() => "new-agent-node-id");
  const mockBuildConversationContext = vi.fn(() => [
    { role: "user", content: "Hello" },
  ]);
  const mockAppendToNode = vi.fn();
  const mockSetStreaming = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  const setupMockStore = (overrides: Record<string, unknown> = {}) => {
    const defaultNodeData = new Map([
      [
        "user-node-1",
        {
          role: "user" as const,
          content: "Test user content",
          timestamp: Date.now(),
        },
      ],
      [
        "agent-node-1",
        {
          role: "assistant" as const,
          content: "Test agent content",
          timestamp: Date.now(),
        },
      ],
    ]);

    mockUseGraphStore.mockImplementation((selector) => {
      const state = {
        previewNodeId: "user-node-1",
        nodeData: overrides.nodeData ?? defaultNodeData,
        streamingNodeId: null,
        setPreviewNode: mockSetPreviewNode,
        updateNodeContent: mockUpdateNodeContent,
        createAgentNodeDownstream: mockCreateAgentNodeDownstream,
        buildConversationContext: mockBuildConversationContext,
        appendToNode: mockAppendToNode,
        setStreaming: mockSetStreaming,
        defaultProvider: "claude-code",
        availableProviders: [
          { provider: "claude-code", available: true, error_message: null },
          { provider: "gemini-cli", available: true, error_message: null },
        ],
        ...overrides,
      };
      return selector(state as unknown as Parameters<typeof selector>[0]);
    });
  };

  describe("Copy button", () => {
    it("shows Copy button when content exists", () => {
      setupMockStore();
      render(<SidePanel />);

      expect(screen.getByRole("button", { name: /copy/i })).toBeInTheDocument();
    });

    it("copies content to clipboard when Copy button is clicked", async () => {
      setupMockStore();
      render(<SidePanel />);

      const copyButton = screen.getByRole("button", { name: /copy/i });
      await userEvent.click(copyButton);

      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
        "Test user content"
      );
    });
  });

  describe("Generate button", () => {
    it("shows Generate button for user nodes when in edit mode", async () => {
      setupMockStore();
      render(<SidePanel />);

      // Enter edit mode first
      const editButton = screen.getByRole("button", { name: /edit/i });
      await userEvent.click(editButton);

      expect(
        screen.getByRole("button", { name: /generate/i })
      ).toBeInTheDocument();
    });

    it("does not show Generate button for agent nodes", async () => {
      setupMockStore({ previewNodeId: "agent-node-1" });
      render(<SidePanel />);

      // Agent nodes don't have an Edit button, so no Generate button
      expect(
        screen.queryByRole("button", { name: /generate/i })
      ).not.toBeInTheDocument();
    });

    it("disables Generate button during streaming", async () => {
      setupMockStore({ streamingNodeId: "some-streaming-node" });
      render(<SidePanel />);

      // Enter edit mode first
      const editButton = screen.getByRole("button", { name: /edit/i });
      await userEvent.click(editButton);

      const generateButton = screen.getByRole("button", { name: /generating/i });
      expect(generateButton).toBeDisabled();
    });

    it("creates downstream agent node when Generate is clicked", async () => {
      setupMockStore();
      render(<SidePanel />);

      // Enter edit mode first
      const editButton = screen.getByRole("button", { name: /edit/i });
      await userEvent.click(editButton);

      const generateButton = screen.getByRole("button", { name: /generate/i });
      await userEvent.click(generateButton);

      // Now takes parentId and provider
      expect(mockCreateAgentNodeDownstream).toHaveBeenCalledWith("user-node-1", "claude-code");
    });
  });

  describe("Keyboard shortcuts", () => {
    it("triggers generation with Cmd+Enter in textarea", async () => {
      setupMockStore();
      render(<SidePanel />);

      // Enter edit mode first
      const editButton = screen.getByRole("button", { name: /edit/i });
      await userEvent.click(editButton);

      // Find textarea and trigger Cmd+Enter
      const textarea = screen.getByRole("textbox");
      fireEvent.keyDown(textarea, { key: "Enter", metaKey: true });

      // Now takes parentId and provider
      expect(mockCreateAgentNodeDownstream).toHaveBeenCalledWith("user-node-1", "claude-code");
    });
  });

  describe("Panel rendering", () => {
    it("renders nothing when no node is selected", () => {
      setupMockStore({ previewNodeId: null });
      const { container } = render(<SidePanel />);

      expect(container.firstChild).toBeNull();
    });

    it("shows User badge for user nodes", () => {
      setupMockStore();
      render(<SidePanel />);

      expect(screen.getByText("User")).toBeInTheDocument();
    });

    it("shows Assistant badge for agent nodes without provider", () => {
      setupMockStore({ previewNodeId: "agent-node-1" });
      render(<SidePanel />);

      expect(screen.getByText("Assistant")).toBeInTheDocument();
    });
  });

  describe("Provider display", () => {
    it('shows "Claude" badge for claude-code provider', () => {
      const nodeData = new Map([
        [
          "agent-node-claude",
          {
            role: "assistant" as const,
            content: "Test content",
            timestamp: Date.now(),
            provider: "claude-code" as const,
          },
        ],
      ]);

      setupMockStore({
        previewNodeId: "agent-node-claude",
        nodeData,
      });
      render(<SidePanel />);

      expect(screen.getByText("Claude")).toBeInTheDocument();
    });

    it('shows "Gemini" badge for gemini-cli provider', () => {
      const nodeData = new Map([
        [
          "agent-node-gemini",
          {
            role: "assistant" as const,
            content: "Test content",
            timestamp: Date.now(),
            provider: "gemini-cli" as const,
          },
        ],
      ]);

      setupMockStore({
        previewNodeId: "agent-node-gemini",
        nodeData,
      });
      render(<SidePanel />);

      expect(screen.getByText("Gemini")).toBeInTheDocument();
    });
  });
});
