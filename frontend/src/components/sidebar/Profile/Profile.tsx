import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
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
//
// Portaled to document.body rather than rendered in place: the sidebar
// has `overflow-x: hidden` and shrinks to 64px when collapsed
// (sidebar.css), so an in-flow `position: absolute` menu (the original
// approach) got squeezed to that same 64px column instead of floating
// over the page at its natural width. Position is computed from the
// trigger's own bounding rect so it still opens in the same visual spot
// (just above the row) regardless of sidebar width/collapsed state.
export function ProfileSwitcher({ displayName, email, onSignOut }: ProfileSwitcherProps) {
  const [open, setOpen] = useState(false);
  const [menuPos, setMenuPos] = useState<{ left: number; bottom: number } | null>(null);
  const triggerRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open || !triggerRef.current) return;
    const rect = triggerRef.current.getBoundingClientRect();
    setMenuPos({ left: rect.left, bottom: window.innerHeight - rect.top + 6 });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (event: MouseEvent) => {
      const target = event.target as Node;
      if (!triggerRef.current?.contains(target) && !menuRef.current?.contains(target)) {
        setOpen(false);
      }
    };
    // Closes on scroll/resize rather than re-tracking the trigger's
    // position live — the trigger essentially never moves while this
    // menu is open in practice (no in-page scroll container it sits
    // in), so a stale-position edge case isn't worth continuous
    // requestAnimationFrame tracking for.
    const handleReflow = () => setOpen(false);
    document.addEventListener('mousedown', handleClickOutside);
    window.addEventListener('resize', handleReflow);
    window.addEventListener('scroll', handleReflow, true);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      window.removeEventListener('resize', handleReflow);
      window.removeEventListener('scroll', handleReflow, true);
    };
  }, [open]);

  return (
    <div className="profile-switcher-wrap">
      {open &&
        menuPos &&
        createPortal(
          <div
            className="profile-menu"
            role="menu"
            ref={menuRef}
            style={{ left: menuPos.left, bottom: menuPos.bottom }}
          >
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
          </div>,
          document.body,
        )}
      <div
        className="profile-switcher"
        ref={triggerRef}
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
