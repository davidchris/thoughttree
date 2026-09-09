// Reduced mark for 16 to 32px. Same grammar as the app icon: filled root,
// outlined children, one merge. Source: design/logo-mark.svg.
export function LogoMark({ size = 24, className }: { size?: number; className?: string }) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 32 32"
      role="img"
      aria-label="ThoughtTree"
      className={className}
    >
      <g fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
        <line x1="16" y1="6" x2="8" y2="15" />
        <line x1="16" y1="6" x2="24" y2="15" />
        <line x1="8" y1="15" x2="16" y2="24" />
        <line x1="24" y1="15" x2="16" y2="24" />
        <circle cx="8" cy="15" r="3" />
        <circle cx="24" cy="15" r="3" />
        <circle cx="16" cy="24" r="3" />
      </g>
      <circle cx="16" cy="6" r="3.5" fill="currentColor" />
    </svg>
  );
}
