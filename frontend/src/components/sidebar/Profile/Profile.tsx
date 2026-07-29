import { useEffect, useRef, useState } from 'react';
import { Avatar } from '../../ui/Avatar/Avatar';
import './profile.css';

interface ProfileSwitcherProps {
  displayName: string;
  email: string;
  onSignOut: () => void;
}

// Clicking the row toggles a small menu above it (email, Account
// placeholder, Log out) — modeled on Claude's own sidebar profile menu.
// "Account" is intentionally inert for now, per instruction; only
// "Log out" is wired.
export function ProfileSwitcher({ displayName, email, onSignOut }: ProfileSwitcherProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (event: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [open]);

  return (
    <div className="profile-switcher-wrap" ref={rootRef}>
      {open && (
        <div className="profile-menu" role="menu">
          <div className="profile-menu-email">{email}</div>
          <button type="button" className="profile-menu-item" role="menuitem">
            Account
          </button>
          <div className="profile-menu-divider" />
          <button
            type="button"
            className="profile-menu-item"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              onSignOut();
            }}
          >
            Log out
          </button>
        </div>
      )}
      <div
        className="profile-switcher"
        role="button"
        tabIndex={0}
        onClick={() => setOpen((value) => !value)}
        title={displayName}
      >
        <Avatar label={displayName} size={34} />
        <span className="profile-name">{displayName}</span>
      </div>
    </div>
  );
}
