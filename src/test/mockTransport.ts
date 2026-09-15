import { vi } from 'vitest';
import type { BackendTransport } from '../lib/transport';

/** Every BackendTransport method as a vi.fn(); pass overrides for the ones a test drives. */
export function createMockTransport(overrides: Partial<BackendTransport> = {}): BackendTransport {
  return {
    capabilities: { nativeDialogs: true },
    loadProject: vi.fn(),
    saveProject: vi.fn(),
    saveProjectCopy: vi.fn(),
    snapshotProject: vi.fn().mockResolvedValue('snapshot-1'),
    listProjectRecovery: vi.fn().mockResolvedValue([]),
    readProjectRecovery: vi.fn(),
    listProjects: vi.fn(),
    importKagiExport: vi.fn(),
    sendPrompt: vi.fn(),
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
    getAvailableModels: vi.fn(),
    generateSummary: vi.fn(),
    onStreamChunk: vi.fn(() => () => {}),
    onPermissionRequest: vi.fn(() => () => {}),
    pickVaultFile: vi.fn(),
    resolveDroppedFile: vi.fn(),
    statVaultFile: vi.fn(),
    readVaultFilePreview: vi.fn(),
    getAttachmentLimits: vi.fn().mockResolvedValue({
      imageMaxBytes: 5 * 1024 * 1024,
      imageMaxSide: 8000,
      maxImagesPerPrompt: 20,
      previewTextBytes: 16 * 1024,
    }),
    ...overrides,
  };
}
