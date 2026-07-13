import { useEffect, useState } from 'react';

const STORAGE_KEY = 'odin:sidebarCollapsed';

function readInitial(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'true';
  } catch {
    return false;
  }
}

// Device-level UI preference, not account-scoped — persists across both
// reload and logout/login (never cleared on sign-out), same as it would
// in a real single-user app where "logging out" is a session boundary,
// not a different person sitting down at the device.
export function useSidebarCollapsed() {
  const [collapsed, setCollapsed] = useState(readInitial);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, String(collapsed));
    } catch {
      // Unavailable (e.g. private browsing) — collapse still works for
      // the session, it just won't persist.
    }
  }, [collapsed]);

  const toggle = () => setCollapsed((prev) => !prev);

  return { collapsed, toggle };
}
