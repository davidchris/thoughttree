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
    const nodeData = new Map([
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
        nodeData,
        streamingNodeId: null,
        setPreviewNode: mockSetPreviewNode,
        updateNodeContent: mockUpdateNodeContent,
        createAgentNodeDownstream: mockCreateAgentNodeDownstream,
        buildConversationContext: mockBuildConversationContext,
        appendToNode: mockAppendToNode,
        setStreaming: mockSetStreaming,
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

      expect(mockCreateAgentNodeDownstream).toHaveBeenCalledWith("user-node-1");
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

      expect(mockCreateAgentNodeDownstream).toHaveBeenCalledWith("user-node-1");
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

    it("shows Assistant badge for agent nodes", () => {
      setupMockStore({ previewNodeId: "agent-node-1" });
      render(<SidePanel />);

      expect(screen.getByText("Assistant")).toBeInTheDocument();
    });
  });
});
