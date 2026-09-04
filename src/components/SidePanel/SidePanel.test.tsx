import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SidePanel } from "./index";
import type { BackendTransport } from "../../lib/transport";
import { setBackendTransport } from "../../lib/transport";
import { useGraphStore } from "../../store/useGraphStore";
import { useUIStore } from "../../store/useUIStore";
import { useProviderStore } from "../../store/useProviderStore";
import type { GraphJSON } from "@thoughttree/graph-model";
import projectV4 from "../../../test/fixtures/project-v4.json";

// Mock the stores
vi.mock("../../store/useGraphStore");
vi.mock("../../store/useUIStore");
vi.mock("../../store/useProviderStore");

const mockUseGraphStore = vi.mocked(useGraphStore);
const mockUseUIStore = vi.mocked(useUIStore);
const mockUseProviderStore = vi.mocked(useProviderStore);
const sanitizedV4Answer = (projectV4.graph as GraphJSON).nodes.find(
  (node) => node.id === "answer"
)!;

function createMockTransport(): BackendTransport {
  return {
    capabilities: { nativeDialogs: true },
    loadProject: vi.fn(),
    saveProject: vi.fn(),
    listProjects: vi.fn(),
    importKagiExport: vi.fn(),
    sendPrompt: vi.fn(() => Promise.resolve("")),
    respondToPermission: vi.fn(),
    checkAcpAvailable: vi.fn(),
    searchFiles: vi.fn(),
    getAvailableProviders: vi.fn(),
    getDefaultProvider: vi.fn(),
    setDefaultProvider: vi.fn(),
    getModelPreferences: vi.fn(),
    setModelPreference: vi.fn(),
    getEffortPreferences: vi.fn(),
    setEffortPreference: vi.fn(),
    getAvailableModels: vi.fn(() => Promise.resolve([])),
    generateSummary: vi.fn(),
    onStreamChunk: vi.fn(() => () => {}),
    onPermissionRequest: vi.fn(() => () => {}),
  };
}

describe("SidePanel", () => {
  let transport: BackendTransport;
  const mockSetPreviewNode = vi.fn();
  const mockUpdateNodeContent = vi.fn();
  const mockCreateAgentNodeDownstream = vi.fn(() => "new-agent-node-id");
  const mockBuildConversationContext = vi.fn(() => [
    { role: "user", content: "Hello" },
  ]);
  const mockAppendToNode = vi.fn();
  const mockStopStreaming = vi.fn();
  const mockIsNodeBlocked = vi.fn(() => false);

  beforeEach(() => {
    vi.clearAllMocks();
    transport = createMockTransport();
    setBackendTransport(transport);
  });

  const mockGetEffectiveModel = vi.fn(() => undefined);
  const mockGetEffectiveEffort = vi.fn(() => "high");
  const mockSetAvailableModels = vi.fn();

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

    // One combined state object serves all three mocked stores; each
    // component selector picks only the fields its store actually owns.
    const state = {
      previewNodeId: "user-node-1",
      nodeData: overrides.nodeData ?? defaultNodeData,
      streamingNodeIds: new Set<string>(),
      setPreviewNode: mockSetPreviewNode,
      updateNodeContent: mockUpdateNodeContent,
      createAgentNodeDownstream: mockCreateAgentNodeDownstream,
      buildConversationContext: mockBuildConversationContext,
      appendToNode: mockAppendToNode,
      stopStreaming: mockStopStreaming,
      isNodeBlocked: overrides.isNodeBlocked ?? mockIsNodeBlocked,
      defaultProvider: "claude-code",
      availableProviders: [
        { provider: "claude-code", available: true, error_message: null },
        { provider: "gemini-cli", available: true, error_message: null },
      ],
      availableModels: {
        "claude-code": [{ model_id: "claude-sonnet", display_name: "Sonnet" }],
        "gemini-cli": [{ model_id: "gemini-3", display_name: "Gemini 3" }],
      },
      getEffectiveModel: mockGetEffectiveModel,
      getEffectiveEffort: mockGetEffectiveEffort,
      setAvailableModels: mockSetAvailableModels,
      triggerSidePanelEdit: false,
      clearSidePanelEditTrigger: vi.fn(),
      ...overrides,
    };

    mockUseGraphStore.mockImplementation((selector) =>
      selector(state as unknown as Parameters<typeof selector>[0])
    );
    mockUseUIStore.mockImplementation((selector) =>
      selector(state as unknown as Parameters<typeof selector>[0])
    );
    mockUseProviderStore.mockImplementation((selector) =>
      selector(state as unknown as Parameters<typeof selector>[0])
    );
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
    it("shows Generate button for user nodes without requiring edit mode", async () => {
      setupMockStore();
      render(<SidePanel />);

      // Generate button should be visible immediately for user nodes
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

    it("disables Generate button when node is blocked", async () => {
      setupMockStore({ isNodeBlocked: vi.fn(() => true) });
      render(<SidePanel />);

      const generateButton = screen.getByRole("button", { name: /generating/i });
      expect(generateButton).toBeDisabled();
    });

    it("creates downstream agent node when Generate is clicked", async () => {
      setupMockStore();
      render(<SidePanel />);

      const generateButton = screen.getByRole("button", { name: /generate/i });
      await userEvent.click(generateButton);

      // Takes parentId, provider, and model
      expect(mockCreateAgentNodeDownstream).toHaveBeenCalledWith("user-node-1", "claude-code", undefined);
    });

    it("passes resolved reasoning effort to prompt generation", async () => {
      setupMockStore();
      render(<SidePanel />);

      await userEvent.click(screen.getByRole("button", { name: /generate/i }));

      await waitFor(() => {
        expect(transport.sendPrompt).toHaveBeenCalledWith({
          nodeId: "new-agent-node-id",
          messages: [{ role: "user", content: "Hello" }],
          provider: "claude-code",
          modelId: undefined,
          effort: "high",
        });
      });
      expect(mockGetEffectiveEffort).toHaveBeenCalledWith("claude-code");
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

      // Takes parentId, provider, and model
      expect(mockCreateAgentNodeDownstream).toHaveBeenCalledWith("user-node-1", "claude-code", undefined);
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
      expect(screen.queryByText(/^Provenance/)).not.toBeInTheDocument();
    });

    it("shows one collapsed Provenance disclosure below the exact answer", () => {
      setupMockStore({
        previewNodeId: "answer",
        nodeData: new Map([["answer", sanitizedV4Answer]]),
      });

      render(<SidePanel />);

      const exactAnswer = screen.getByText("The exact assistant answer stays unchanged.");
      const disclosure = screen.getByText("Provenance · 1 source · 2 files · 4 activities");

      expect(disclosure.closest("details")).not.toHaveAttribute("open");
      expect(exactAnswer.compareDocumentPosition(disclosure)).toBe(
        Node.DOCUMENT_POSITION_FOLLOWING
      );
      expect(screen.getAllByText(/^Provenance/)).toHaveLength(1);
    });

    it("renders canonical references first with safe links and visible relations", async () => {
      setupMockStore({
        previewNodeId: "answer",
        nodeData: new Map([["answer", sanitizedV4Answer]]),
      });
      render(<SidePanel />);

      await userEvent.click(screen.getByText(/^Provenance/));

      const source = screen.getByRole("link", { name: "Canonical evidence" });
      const vaultFile = screen.getByText("research/evidence.md");
      const externalFile = screen.getByText("outside-notes.txt");
      const activityHeading = screen.getByRole("heading", { name: "Turn activity" });

      expect(source).toHaveAttribute(
        "href",
        "https://example.com/evidence?item=1#result"
      );
      expect(screen.getByText("consulted · cited")).toBeInTheDocument();
      expect(screen.getByText("read · cited")).toBeInTheDocument();
      expect(screen.getByText("read")).toBeInTheDocument();
      expect(vaultFile.closest("a")).toBeNull();
      expect(externalFile.closest("a")).toBeNull();
      expect(source.compareDocumentPosition(vaultFile)).toBe(
        Node.DOCUMENT_POSITION_FOLLOWING
      );
      expect(vaultFile.compareDocumentPosition(externalFile)).toBe(
        Node.DOCUMENT_POSITION_FOLLOWING
      );
      expect(externalFile.compareDocumentPosition(activityHeading)).toBe(
        Node.DOCUMENT_POSITION_FOLLOWING
      );
    });

    it("resolves Kagi citation markers to ordered source references without changing the answer", async () => {
      setupMockStore({
        previewNodeId: "kagi-answer",
        nodeData: new Map([
          [
            "kagi-answer",
            {
              id: "kagi-answer",
              role: "assistant" as const,
              content: "The fetched page supports this claim【2】 and the search result supports that claim【1】.",
              timestamp: 1,
              provenance: {
                completeness: "complete" as const,
                references: [
                  {
                    type: "url" as const,
                    url: "https://example.com/uncited",
                    title: "Uncited source",
                    index: 3,
                    is_search_result: true,
                    relations: ["consulted" as const],
                  },
                  {
                    type: "url" as const,
                    url: "https://example.com/page",
                    title: "Fetched source",
                    index: 2,
                    is_search_result: false,
                    relations: ["consulted" as const],
                  },
                  {
                    type: "url" as const,
                    url: "https://example.com/search",
                    title: "Search source",
                    index: 1,
                    is_search_result: true,
                    relations: ["consulted" as const],
                  },
                ],
                activity: [],
              },
            },
          ],
        ]),
      });
      render(<SidePanel />);

      expect(
        screen.getByText(
          "The fetched page supports this claim【2】 and the search result supports that claim【1】."
        )
      ).toBeInTheDocument();
      await userEvent.click(screen.getByText(/^Provenance/));

      const references = screen.getAllByRole("listitem").slice(0, 3);
      expect(references[0]).toHaveTextContent("Search source");
      expect(references[0]).toHaveTextContent("Cited");
      expect(references[0]).toHaveTextContent("Search result");
      expect(references[1]).toHaveTextContent("Fetched source");
      expect(references[1]).toHaveTextContent("Cited");
      expect(references[1]).toHaveTextContent("Fetched page");
      expect(references[2]).toHaveTextContent("Uncited source");
      expect(references[2]).toHaveTextContent("Consulted");
      expect(references[2]).not.toHaveTextContent("Cited");
    });

    it("collapses provenance disclosures again when a different node is previewed", async () => {
      const provenanceNode = (id: string, content: string) => ({
        id,
        role: "assistant" as const,
        content,
        timestamp: 1,
        provenance: {
          completeness: "complete" as const,
          references: [],
          activity: [
            { type: "commentary" as const, content: `${id} commentary` },
          ],
        },
      });
      const nodeData = new Map([
        ["answer-a", provenanceNode("answer-a", "Answer A")],
        ["answer-b", provenanceNode("answer-b", "Answer B")],
      ]);
      setupMockStore({ previewNodeId: "answer-a", nodeData });
      const { rerender } = render(<SidePanel />);

      await userEvent.click(screen.getByText(/^Provenance/));
      await userEvent.click(screen.getByText("Assistant commentary"));
      const openBefore = screen.getByText(/^Provenance/).closest("details");
      expect(openBefore).toHaveAttribute("open");
      expect(screen.getByText("Assistant commentary").closest("details")).toHaveAttribute("open");

      setupMockStore({ previewNodeId: "answer-b", nodeData });
      rerender(<SidePanel />);

      expect(screen.getByText("Answer B")).toBeInTheDocument();
      expect(screen.getByText(/^Provenance/).closest("details")).not.toHaveAttribute("open");
      expect(screen.getByText("Assistant commentary").closest("details")).not.toHaveAttribute("open");
    });

    it("reports citation markers with no matching reference", async () => {
      setupMockStore({
        previewNodeId: "dangling-answer",
        nodeData: new Map([
          [
            "dangling-answer",
            {
              id: "dangling-answer",
              role: "assistant" as const,
              content: "This claim has no source【9】.",
              timestamp: 1,
              provenance: {
                completeness: "complete" as const,
                references: [],
                activity: [],
              },
            },
          ],
        ]),
      });
      render(<SidePanel />);

      expect(screen.getByText("This claim has no source【9】.")).toBeInTheDocument();
      await userEvent.click(screen.getByText(/^Provenance/));

      expect(screen.getByText("Citation marker 【9】 has no matching reference.")).toBeInTheDocument();
    });

    it("warns about partial evidence and preserves authoritative activity order", async () => {
      setupMockStore({
        previewNodeId: "answer",
        nodeData: new Map([["answer", sanitizedV4Answer]]),
      });
      render(<SidePanel />);

      await userEvent.click(screen.getByText(/^Provenance/));

      expect(
        screen.getByText("Some Turn evidence may be missing.")
      ).toBeInTheDocument();

      const commentary = screen.getAllByText("Assistant commentary");
      const tool = screen.getByText("Read · Completed");
      const unknown = screen.getByText("provider_status · Provider status update");

      expect(commentary).toHaveLength(2);
      expect(commentary[0].closest("details")).not.toHaveAttribute("open");
      expect(tool.closest("details")).not.toHaveAttribute("open");
      expect(screen.getByText("Read the sanitized evidence fixt…")).toBeInTheDocument();
      expect(screen.getByText("Title truncated")).toBeInTheDocument();
      expect(commentary[0].compareDocumentPosition(tool)).toBe(
        Node.DOCUMENT_POSITION_FOLLOWING
      );
      expect(tool.compareDocumentPosition(commentary[1])).toBe(
        Node.DOCUMENT_POSITION_FOLLOWING
      );
      expect(commentary[1].compareDocumentPosition(unknown)).toBe(
        Node.DOCUMENT_POSITION_FOLLOWING
      );
    });

    it("renders untrusted historical provenance as bounded plain text", async () => {
      const longTitle = `<button>Run me</button>${"x".repeat(220)}`;
      const displayedTitle = longTitle.slice(0, 200);
      setupMockStore({
        previewNodeId: "unsafe-answer",
        nodeData: new Map([
          [
            "unsafe-answer",
            {
              id: "unsafe-answer",
              role: "assistant" as const,
              content: "Safe answer",
              timestamp: 1,
              provenance: {
                completeness: "unknown" as const,
                references: [
                  {
                    type: "url" as const,
                    url: "javascript:alert('no')",
                    title: "Unsafe URL",
                    relations: ["consulted" as const],
                  },
                  {
                    type: "url" as const,
                    url: "mailto:person@example.com",
                    title: "Email URL",
                    relations: ["consulted" as const],
                  },
                  {
                    type: "url" as const,
                    url: "http://example.com",
                    title: "HTTP URL",
                    relations: ["consulted" as const],
                  },
                ],
                activity: [
                  {
                    type: "tool" as const,
                    kind: "execute" as const,
                    title: longTitle,
                    status: "incomplete" as const,
                  },
                  {
                    type: "tool" as const,
                    kind: "read" as const,
                    title: "Read a file",
                    titleRedacted: true,
                    status: "completed" as const,
                  },
                  {
                    type: "unknown" as const,
                    providerType: "future_item",
                    label: "Future provider item",
                    rawPayload: "SECRET RAW PAYLOAD",
                  },
                ],
              },
            },
          ],
        ]),
      });
      render(<SidePanel />);

      await userEvent.click(screen.getByText(/^Provenance/));

      expect(screen.getByText("Some Turn evidence may be missing.")).toBeInTheDocument();
      expect(screen.getByText("Unsafe URL").closest("a")).toBeNull();
      expect(screen.getByText("Email URL").closest("a")).toBeNull();
      expect(screen.getByRole("link", { name: "HTTP URL" })).toHaveAttribute(
        "href",
        "http://example.com"
      );
      expect(screen.getByText("Execute · Incomplete").closest("details")).not.toHaveAttribute(
        "open"
      );
      expect(screen.getByText(displayedTitle)).toBeInTheDocument();
      expect(screen.getByText("Title truncated")).toBeInTheDocument();
      expect(screen.getByText("Read a file")).toBeInTheDocument();
      expect(screen.getByText("Title replaced by a summary")).toBeInTheDocument();
      expect(screen.getByText("future_item · Future provider item")).toBeInTheDocument();
      expect(screen.queryByText("SECRET RAW PAYLOAD")).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Run me" })).not.toBeInTheDocument();
    });

    it("distinguishes a complete empty Turn from incomplete evidence", async () => {
      setupMockStore({
        previewNodeId: "empty-answer",
        nodeData: new Map([
          [
            "empty-answer",
            {
              id: "empty-answer",
              role: "assistant" as const,
              content: "",
              timestamp: 1,
              provenance: {
                completeness: "complete" as const,
                references: [],
                activity: [],
              },
            },
          ],
        ]),
      });
      render(<SidePanel />);

      expect(screen.getByText("No content")).toBeInTheDocument();
      await userEvent.click(
        screen.getByText("Provenance · 0 sources · 0 files · 0 activities")
      );

      expect(screen.getByText("No references recorded.")).toBeInTheDocument();
      expect(screen.getByText("No Turn activity recorded.")).toBeInTheDocument();
      expect(screen.queryByText("Some Turn evidence may be missing.")).not.toBeInTheDocument();
    });
  });

  describe("Model selection", () => {
    it("offers the Codex model list when Codex is the active provider", () => {
      setupMockStore({
        defaultProvider: "codex",
        availableProviders: [
          { provider: "claude-code", available: true, error_message: null },
          { provider: "codex", available: true, error_message: null },
        ],
        availableModels: {
          "claude-code": [{ model_id: "claude-sonnet", display_name: "Sonnet" }],
          codex: [
            { model_id: "gpt-5.5", display_name: "GPT-5.5" },
            { model_id: "gpt-5.4-mini", display_name: "GPT-5.4 Mini" },
          ],
        },
      });
      render(<SidePanel />);

      expect(
        screen.getByRole("option", { name: "GPT-5.5" })
      ).toBeInTheDocument();
      expect(
        screen.getByRole("option", { name: "GPT-5.4 Mini" })
      ).toBeInTheDocument();
      // Claude's models must not leak into the Codex selector
      expect(
        screen.queryByRole("option", { name: "Sonnet" })
      ).not.toBeInTheDocument();
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
