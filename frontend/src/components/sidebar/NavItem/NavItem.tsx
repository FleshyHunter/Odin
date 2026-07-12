import type { KeyboardEvent, ReactNode } from 'react';
import './navItem.css';

interface NavItemProps {
  icon?: ReactNode;
  label: string;
  active?: boolean;
  onClick?: () => void;
  /** CSS class family to render — "nav-item" (Tracks/Projects/Pinned/Just
   *  ask a question) or "recent-item" (RecentsList), which have distinct
   *  active-state styling (plain surface vs gold/glow-dim). */
  baseClassName?: 'nav-item' | 'recent-item';
}

export function NavItem({ icon, label, active, onClick, baseClassName = 'nav-item' }: NavItemProps) {
  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onClick?.();
    }
  };

  return (
    <div
      className={active ? `${baseClassName} active` : baseClassName}
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={handleKeyDown}
    >
      {icon}
      {label}
    </div>
  );
}
