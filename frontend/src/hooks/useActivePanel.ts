import { useEffect, useState } from 'react';

const WIDTH_KEY = 'odin:activePanelWidth';
const OPEN_KEY = 'odin:activePanelOpen';

export const ACTIVE_PANEL_MIN_WIDTH = 320;
const DEFAULT_WIDTH = 320;

function readWidth(): number {
  try {
    const stored = Number(localStorage.getItem(WIDTH_KEY));
    return stored >= ACTIVE_PANEL_MIN_WIDTH ? stored : DEFAULT_WIDTH;
  } catch {
    return DEFAULT_WIDTH;
  }
}

function readOpen(): boolean {
  try {
    return localStorage.getItem(OPEN_KEY) !== 'false';
  } catch {
    return true;
  }
}

// Device-level UI preference, same reasoning as useSidebarCollapsed —
// persists across reload and logout/login, not scoped to the auth session.
export function useActivePanel() {
  const [width, setWidth] = useState(readWidth);
  const [isOpen, setIsOpen] = useState(readOpen);

  useEffect(() => {
    try {
      localStorage.setItem(WIDTH_KEY, String(width));
    } catch {
      // Unavailable (e.g. private browsing) — resize still works for the
      // session, it just won't persist.
    }
  }, [width]);

  useEffect(() => {
    try {
      localStorage.setItem(OPEN_KEY, String(isOpen));
    } catch {
      // Same as above.
    }
  }, [isOpen]);

  const toggle = () => setIsOpen((prev) => !prev);

  return { width, setWidth, isOpen, toggle };
}
