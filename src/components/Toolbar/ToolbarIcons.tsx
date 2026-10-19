import type { SVGProps } from 'react';

// UI icons per design/DESIGN.md: line style, 16px, 1.5px stroke, currentColor.
function Icon({ children, ...rest }: SVGProps<SVGSVGElement>) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...rest}
    >
      {children}
    </svg>
  );
}

export const NewIcon = () => (
  <Icon>
    <path d="M9 2H4.5A1.5 1.5 0 0 0 3 3.5v9A1.5 1.5 0 0 0 4.5 14h7a1.5 1.5 0 0 0 1.5-1.5V6z" />
    <path d="M9 2v4h4" />
    <path d="M8 8v4M6 10h4" />
  </Icon>
);

export const OpenIcon = () => (
  <Icon>
    <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v6a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12z" />
  </Icon>
);

export const ImportIcon = () => (
  <Icon>
    <path d="M8 2v8" />
    <path d="M5 7l3 3 3-3" />
    <path d="M3 11v1.5A1.5 1.5 0 0 0 4.5 14h7a1.5 1.5 0 0 0 1.5-1.5V11" />
  </Icon>
);

export const SaveIcon = () => (
  <Icon>
    <path d="M3 3.5A1.5 1.5 0 0 1 4.5 2h6L13 4.5v8a1.5 1.5 0 0 1-1.5 1.5h-7A1.5 1.5 0 0 1 3 12.5z" />
    <path d="M5.5 14v-4h5v4" />
    <path d="M5.5 2v3h4" />
  </Icon>
);

export const TidyIcon = () => (
  <Icon>
    <rect x="6" y="2" width="4" height="3" rx="0.75" />
    <rect x="2" y="11" width="4" height="3" rx="0.75" />
    <rect x="10" y="11" width="4" height="3" rx="0.75" />
    <path d="M8 5v2.5M8 7.5H4v3.5M8 7.5h4v3.5" />
  </Icon>
);

export const ReplyIcon = () => (
  <Icon>
    <path d="M6.5 4L3 7.5 6.5 11" />
    <path d="M3 7.5h6a4 4 0 0 1 4 4V13" />
  </Icon>
);

export const ExportThreadIcon = () => (
  <Icon>
    <path d="M8 10V2" />
    <path d="M5 5l3-3 3 3" />
    <path d="M3 9v3.5A1.5 1.5 0 0 0 4.5 14h7a1.5 1.5 0 0 0 1.5-1.5V9" />
  </Icon>
);

export const ExportAllIcon = () => (
  <Icon>
    <path d="M2 8l6 3 6-3" />
    <path d="M2 11l6 3 6-3" />
    <path d="M2 5l6-3 6 3-6 3z" />
  </Icon>
);

export const HistoryIcon = () => (
  <Icon>
    <path d="M2.5 8a5.5 5.5 0 1 0 1.6-3.9" />
    <path d="M2.5 2.5v3.5H6" />
    <path d="M8 5.5V8l2 1.5" />
  </Icon>
);

export const SettingsIcon = () => (
  <Icon>
    <path d="M2 4.5h7M12 4.5h2" />
    <circle cx="10.5" cy="4.5" r="1.5" />
    <path d="M2 11.5h2M7 11.5h7" />
    <circle cx="5.5" cy="11.5" r="1.5" />
  </Icon>
);
