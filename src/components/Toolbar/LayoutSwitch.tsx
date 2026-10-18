import { LAYOUT_STYLES, type LayoutStyle } from '../../lib/layout';
import { LayoutGlyph } from './ToolbarIcons';

interface LayoutSwitchProps {
  value: LayoutStyle;
  /** Called when a segment is pressed, including the one already selected. */
  onPick: (style: LayoutStyle) => void;
}

/**
 * Segmented control for the Tidy style. Each segment shows a pictogram of
 * its layout; pressing one tidies the graph with it.
 */
export function LayoutSwitch({ value, onPick }: LayoutSwitchProps) {
  return (
    <div className="layout-switch" role="group" aria-label="Tidy layout">
      {LAYOUT_STYLES.map(({ id, label, hint }) => (
        <button
          key={id}
          type="button"
          className="layout-switch-option"
          aria-pressed={id === value}
          aria-label={label}
          title={`${label}: ${hint}`}
          onClick={() => onPick(id)}
        >
          <LayoutGlyph style={id} />
        </button>
      ))}
    </div>
  );
}
