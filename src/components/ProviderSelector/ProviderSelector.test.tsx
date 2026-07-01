import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ProviderSelector } from './index';
import type { ProviderStatus } from '../../types';

describe('ProviderSelector', () => {
  const mockOnChange = vi.fn();

  const allProvidersAvailable: ProviderStatus[] = [
    { provider: 'claude-code', available: true, error_message: null },
    { provider: 'gemini-cli', available: true, error_message: null },
  ];

  const geminiUnavailable: ProviderStatus[] = [
    { provider: 'claude-code', available: true, error_message: null },
    {
      provider: 'gemini-cli',
      available: false,
      error_message: 'Gemini CLI not found',
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders all available providers', () => {
    render(
      <ProviderSelector
        value="claude-code"
        onChange={mockOnChange}
        availableProviders={allProvidersAvailable}
      />
    );

    const select = screen.getByRole('combobox');
    expect(select).toBeInTheDocument();

    // Check both options exist
    const options = screen.getAllByRole('option');
    expect(options).toHaveLength(2);
    expect(options[0]).toHaveTextContent('Claude');
    expect(options[1]).toHaveTextContent('Gemini');
  });

  it('shows current selection', () => {
    render(
      <ProviderSelector
        value="gemini-cli"
        onChange={mockOnChange}
        availableProviders={allProvidersAvailable}
      />
    );

    const select = screen.getByRole('combobox') as HTMLSelectElement;
    expect(select.value).toBe('gemini-cli');
  });

  it('calls onChange when selection changes', async () => {
    const user = userEvent.setup();

    render(
      <ProviderSelector
        value="claude-code"
        onChange={mockOnChange}
        availableProviders={allProvidersAvailable}
      />
    );

    const select = screen.getByRole('combobox');
    await user.selectOptions(select, 'gemini-cli');

    expect(mockOnChange).toHaveBeenCalledWith('gemini-cli');
  });

  it('disables unavailable providers', () => {
    render(
      <ProviderSelector
        value="claude-code"
        onChange={mockOnChange}
        availableProviders={geminiUnavailable}
      />
    );

    const options = screen.getAllByRole('option');
    const claudeOption = options.find((opt) =>
      opt.textContent?.includes('Claude')
    );
    const geminiOption = options.find((opt) =>
      opt.textContent?.includes('Gemini')
    );

    expect(claudeOption).not.toBeDisabled();
    expect(geminiOption).toBeDisabled();
  });

  it('shows unavailable indicator for disabled providers', () => {
    render(
      <ProviderSelector
        value="claude-code"
        onChange={mockOnChange}
        availableProviders={geminiUnavailable}
      />
    );

    // Check that unavailable provider shows indicator
    expect(screen.getByText(/unavailable/i)).toBeInTheDocument();
  });

  it('offers Codex and disables it with the install hint when the adapter is missing', () => {
    const codexUnavailable: ProviderStatus[] = [
      { provider: 'claude-code', available: true, error_message: null },
      {
        provider: 'codex',
        available: false,
        error_message:
          'Codex not found. Install adapter: npm install -g @zed-industries/codex-acp — then login: npm install -g @openai/codex && codex login',
      },
    ];

    render(
      <ProviderSelector
        value="claude-code"
        onChange={mockOnChange}
        availableProviders={codexUnavailable}
      />
    );

    const codexOption = screen
      .getAllByRole('option')
      .find((opt) => opt.textContent?.includes('Codex'));

    expect(codexOption).toBeDisabled();
    expect(codexOption).toHaveAttribute(
      'title',
      expect.stringContaining('@zed-industries/codex-acp')
    );
  });

  it('lets the user select Codex when the adapter is available', async () => {
    const user = userEvent.setup();
    const codexAvailable: ProviderStatus[] = [
      { provider: 'claude-code', available: true, error_message: null },
      { provider: 'codex', available: true, error_message: null },
    ];

    render(
      <ProviderSelector
        value="claude-code"
        onChange={mockOnChange}
        availableProviders={codexAvailable}
      />
    );

    await user.selectOptions(screen.getByRole('combobox'), 'codex');

    expect(mockOnChange).toHaveBeenCalledWith('codex');
  });

  it('applies compact styling when compact prop is true', () => {
    render(
      <ProviderSelector
        value="claude-code"
        onChange={mockOnChange}
        availableProviders={allProvidersAvailable}
        compact
      />
    );

    const select = screen.getByRole('combobox');
    expect(select).toHaveClass('compact');
  });

  it('is disabled when disabled prop is true', () => {
    render(
      <ProviderSelector
        value="claude-code"
        onChange={mockOnChange}
        availableProviders={allProvidersAvailable}
        disabled
      />
    );

    const select = screen.getByRole('combobox');
    expect(select).toBeDisabled();
  });
});
