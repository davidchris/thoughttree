import type { ButtonHTMLAttributes, ReactNode } from 'react';

interface ToolbarButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon: ReactNode;
  /** Visible label. Hidden on narrow toolbars, where `title` carries it. */
  label: string;
}

// Ghost toolbar button: icon plus label. Below the container breakpoint in
// Toolbar.css the label collapses and only the icon remains.
export function ToolbarButton({ icon, label, title, className, ...rest }: ToolbarButtonProps) {
  return (
    <button
      type="button"
      title={title ?? label}
      aria-label={label}
      className={['toolbar-button', className].filter(Boolean).join(' ')}
      {...rest}
    >
      <span className="toolbar-icon">{icon}</span>
      <span className="toolbar-label">{label}</span>
    </button>
  );
}
