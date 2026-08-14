import type { ReactNode } from 'react';
import './tooltip.css';

interface TooltipProps {
  label: string;
  children: ReactNode;
}

// Generic hover label — centered under whatever it wraps, shown via a
// pure CSS :hover toggle (no JS state needed for the show/hide itself).
// First real use: MessageUtil's copy/timestamp icons.
export function Tooltip({ label, children }: TooltipProps) {
  return (
    <span className="tooltip-anchor">
      {children}
      <span className="tooltip-bubble" role="tooltip">
        {label}
      </span>
    </span>
  );
}
