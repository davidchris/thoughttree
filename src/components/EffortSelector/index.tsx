import {
  PROVIDER_DISPLAY_NAMES,
  PROVIDER_SUPPORTED_EFFORTS,
  type AgentProvider,
  type ReasoningEffort,
} from '../../types';
import '../ModelSelector/styles.css';

interface EffortSelectorProps {
  provider: AgentProvider;
  value: ReasoningEffort | undefined;
  onChange: (effort: ReasoningEffort | null) => void;
}

const EFFORT_LABELS: Record<ReasoningEffort, string> = {
  low: 'Low',
  medium: 'Medium',
  high: 'High',
  xhigh: 'XHigh',
};

export function EffortSelector({ provider, value, onChange }: EffortSelectorProps) {
  const supportedEfforts = PROVIDER_SUPPORTED_EFFORTS[provider];
  if (supportedEfforts.length === 0) {
    return null;
  }

  return (
    <select
      className="model-selector"
      value={value ?? ''}
      onChange={(event) =>
        onChange(event.target.value ? (event.target.value as ReasoningEffort) : null)
      }
      aria-label={`${PROVIDER_DISPLAY_NAMES[provider]} reasoning effort`}
      title="Select reasoning effort"
    >
      <option value="">Default</option>
      {supportedEfforts.map((effort) => (
        <option key={effort} value={effort}>
          {EFFORT_LABELS[effort]}
        </option>
      ))}
    </select>
  );
}
