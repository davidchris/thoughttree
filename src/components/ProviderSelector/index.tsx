import type { AgentProvider, ProviderStatus } from '../../types';
import { PROVIDER_SHORT_NAMES } from '../../types';
import './styles.css';

interface ProviderSelectorProps {
  value: AgentProvider;
  onChange: (provider: AgentProvider) => void;
  availableProviders: ProviderStatus[];
  disabled?: boolean;
  compact?: boolean;
}

export function ProviderSelector({
  value,
  onChange,
  availableProviders,
  disabled,
  compact,
}: ProviderSelectorProps) {
  return (
    <select
      className={`provider-selector ${compact ? 'compact' : ''}`}
      value={value}
      onChange={(e) => onChange(e.target.value as AgentProvider)}
      disabled={disabled}
      title="Select AI provider"
    >
      {availableProviders.map((status) => (
        <option
          key={status.provider}
          value={status.provider}
          disabled={!status.available}
          title={status.error_message ?? undefined}
        >
          {PROVIDER_SHORT_NAMES[status.provider]}
          {!status.available && ' (unavailable)'}
        </option>
      ))}
    </select>
  );
}
