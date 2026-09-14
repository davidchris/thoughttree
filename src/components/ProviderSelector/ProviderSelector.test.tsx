import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ProviderSelector } from './index';
import type { ProviderStatus } from '../../types';

describe('ProviderSelector', () => {
  const mockOnChange = vi.fn();

  const allProvidersAvailable: ProviderStatus[] = [
    { provider: 'claude-code', available: true, error_message: null },
    { provider: 'codex', available: true, error_message: null },
  ];

  const codexUnavailable: ProviderStatus[] = [
    { provider: 'claude-code', available: true, error_message: null },
    {
      provider: 'codex',
      available: false,
      error_message: 'Codex not found',
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
    expect(options[1]).toHaveTextContent('Codex');
  });

  it('shows current selection', () => {
    render(
      <ProviderSelector
        value="codex"
        onChange={mockOnChange}
        availableProviders={allProvidersAvailable}
      />
    );

    const select = screen.getByRole('combobox') as HTMLSelectElement;
    expect(select.value).toBe('codex');
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
    await user.selectOptions(select, 'codex');

    expect(mockOnChange).toHaveBeenCalledWith('codex');
  });

  it('disables unavailable providers', () => {
    render(
      <ProviderSelector
        value="claude-code"
        onChange={mockOnChange}
        availableProviders={codexUnavailable}
      />
    );

    const options = screen.getAllByRole('option');
    const claudeOption = options.find((opt) =>
      opt.textContent?.includes('Claude')
    );
    const codexOption = options.find((opt) =>
      opt.textContent?.includes('Codex')
    );

    expect(claudeOption).not.toBeDisabled();
    expect(codexOption).toBeDisabled();
  });

  it('shows unavailable indicator for disabled providers', () => {
    render(
      <ProviderSelector
        value="claude-code"
        onChange={mockOnChange}
        availableProviders={codexUnavailable}
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
          'Codex not found. Install adapter: npm install -g @agentclientprotocol/codex-acp — then login: npm install -g @openai/codex && codex login',
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
      expect.stringContaining('@agentclientprotocol/codex-acp')
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
