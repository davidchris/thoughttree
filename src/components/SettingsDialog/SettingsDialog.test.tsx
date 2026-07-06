import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SettingsDialog } from './index';
import { useGraphStore } from '../../store/useGraphStore';
import { useProviderStore } from '../../store/useProviderStore';
import {
  getAvailableModels,
  getEffortPreferences,
  getModelPreferences,
  getProviderPaths,
  setEffortPreference,
} from '../../lib/tauri';

vi.mock('../../lib/tauri', () => ({
  getAvailableModels: vi.fn(),
  getAvailableProviders: vi.fn(),
  getEffortPreferences: vi.fn(),
  getModelPreferences: vi.fn(),
  getProviderPaths: vi.fn(),
  pickProviderExecutable: vi.fn(),
  setEffortPreference: vi.fn(),
  setModelPreference: vi.fn(),
  setProviderPath: vi.fn(),
  validateProviderPath: vi.fn(),
}));

describe('SettingsDialog reasoning effort controls', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useGraphStore.getState().newProject();
    useGraphStore.setState({ projectPath: '/tmp/project.thoughttree' });
    useProviderStore.setState({
      availableProviders: [
        { provider: 'claude-code', available: true, error_message: null },
        { provider: 'gemini-cli', available: true, error_message: null },
        { provider: 'codex', available: true, error_message: null },
      ],
      globalModelPreferences: {},
      globalEffortPreferences: {},
      availableModels: {
        'claude-code': [{ model_id: 'claude-sonnet', display_name: 'Sonnet' }],
        'gemini-cli': [{ model_id: 'gemini-3', display_name: 'Gemini 3' }],
        codex: [{ model_id: 'gpt-5.5', display_name: 'GPT-5.5' }],
      },
    });

    vi.mocked(getModelPreferences).mockResolvedValue({});
    vi.mocked(getEffortPreferences).mockResolvedValue({});
    vi.mocked(getProviderPaths).mockResolvedValue({});
    vi.mocked(getAvailableModels).mockResolvedValue([]);
    vi.mocked(setEffortPreference).mockResolvedValue();
  });

  it('renders supported efforts and persists global/project selections', async () => {
    const user = userEvent.setup();
    render(<SettingsDialog isOpen onClose={vi.fn()} />);

    const codexEffortSelects = await screen.findAllByLabelText('Codex reasoning effort');
    expect(codexEffortSelects).toHaveLength(2);
    expect(screen.getAllByRole('option', { name: 'Default' }).length).toBeGreaterThanOrEqual(2);
    expect(screen.getAllByRole('option', { name: 'Low' }).length).toBeGreaterThanOrEqual(2);
    expect(screen.getAllByRole('option', { name: 'Medium' }).length).toBeGreaterThanOrEqual(2);
    expect(screen.getAllByRole('option', { name: 'High' }).length).toBeGreaterThanOrEqual(2);
    expect(screen.getAllByRole('option', { name: 'XHigh' }).length).toBeGreaterThanOrEqual(2);
    expect(screen.queryByLabelText('Gemini CLI reasoning effort')).not.toBeInTheDocument();

    await user.selectOptions(codexEffortSelects[0], 'high');

    await waitFor(() => {
      expect(setEffortPreference).toHaveBeenCalledWith('codex', 'high');
    });
    expect(useProviderStore.getState().globalEffortPreferences.codex).toBe('high');

    await user.selectOptions(codexEffortSelects[1], 'xhigh');

    expect(useGraphStore.getState().projectEffortPreferences).toEqual({ codex: 'xhigh' });
  });
});
