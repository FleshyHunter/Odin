import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import * as authApi from '../api/auth';
import { setAccessToken } from '../api/client';
import type { AuthSession } from '../types';

interface AuthContextValue {
  session: AuthSession | null;
  // True only for the brief silent-refresh check on first app load —
  // lets App.tsx avoid flashing the sign-in page before that check
  // resolves (see the bootstrap effect below).
  isBootstrapping: boolean;
  signIn: (email: string, password: string) => Promise<AuthSession>;
  requestLoginOtp: (email: string) => Promise<void>;
  verifyLoginOtp: (email: string, code: string) => Promise<AuthSession>;
  requestSignupOtp: (email: string) => Promise<void>;
  verifySignupOtp: (email: string, code: string) => Promise<void>;
  completeSignup: (email: string, displayName: string, password: string) => Promise<AuthSession>;
  requestPasswordResetOtp: (email: string) => Promise<void>;
  verifyPasswordResetOtp: (email: string, code: string) => Promise<string>;
  completePasswordReset: (email: string, newPassword: string) => Promise<AuthSession>;
  signOut: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

// A real Context (not a plain hook) specifically because `session` is
// genuinely global: the sidebar (email display, log out) and every
// auth-flow component (sign-in, signup wizard, forgot-password wizard)
// all need the SAME session, not each independently maintaining and
// silently drifting out of sync with its own local copy.
//
// isLoading/error are deliberately NOT part of this shared state (they
// were originally, and that was a real bug: every consumer reads the
// same context value, so one screen's "invalid code" error — or its
// loading spinner — kept showing up on completely unrelated screens
// after navigating away, since nothing had scoped it back down). Each
// calling component tracks its own isLoading/error locally around
// calling these functions, exactly like a normal async call — this
// object only exposes the actions + the one genuinely-shared value.
export function AuthProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<AuthSession | null>(null);
  const [isBootstrapping, setIsBootstrapping] = useState(true);
  // Since the refresh call below ROTATES the cookie (a new one replaces
  // it on every use), React StrictMode's dev-only double-invoke of
  // effects would otherwise fire two near-simultaneous rotations on
  // every load — harmless (one ends up an orphaned-but-valid row, not a
  // lockout) but wasteful. Standard guard for "this effect must only
  // truly run once, even under StrictMode."
  const hasBootstrapped = useRef(false);

  // Runs once on app load: silently try to restore a session from the
  // httpOnly refresh cookie. The backend rotates that cookie and resets
  // its 30-day window on every use (sliding expiry, backend/src/auth/
  // handlers.rs), so as long as the app gets opened at least once
  // within any 30-day span, this succeeds and the user never sees a
  // login screen — only real, sustained inactivity ever lets the cookie
  // actually expire.
  useEffect(() => {
    if (hasBootstrapped.current) return;
    hasBootstrapped.current = true;
    (async () => {
      try {
        const restored = await authApi.restoreSession();
        setSession(restored);
      } catch {
        // No valid cookie — never logged in, expired, or revoked. The
        // normal, expected case for a first-time or logged-out visitor,
        // not a real error to surface.
      } finally {
        setIsBootstrapping(false);
      }
    })();
  }, []);

  const signIn = useCallback(async (email: string, password: string) => {
    const result = await authApi.signIn(email, password);
    setSession(result);
    return result;
  }, []);

  const requestLoginOtp = useCallback((email: string) => authApi.requestLoginOtp(email), []);

  const verifyLoginOtp = useCallback(async (email: string, code: string) => {
    const result = await authApi.verifyLoginOtp(email, code);
    setSession(result);
    return result;
  }, []);

  const requestSignupOtp = useCallback((email: string) => authApi.requestSignupOtp(email), []);

  const verifySignupOtp = useCallback(
    (email: string, code: string) => authApi.verifySignupOtp(email, code),
    [],
  );

  const completeSignup = useCallback(async (email: string, displayName: string, password: string) => {
    const result = await authApi.completeSignup(email, displayName, password);
    setSession(result);
    return result;
  }, []);

  const requestPasswordResetOtp = useCallback(
    (email: string) => authApi.requestPasswordResetOtp(email),
    [],
  );

  const verifyPasswordResetOtp = useCallback(
    (email: string, code: string) => authApi.verifyPasswordResetOtp(email, code),
    [],
  );

  const completePasswordReset = useCallback(async (email: string, newPassword: string) => {
    const result = await authApi.completePasswordReset(email, newPassword);
    setSession(result);
    return result;
  }, []);

  const signOut = useCallback(async () => {
    try {
      await authApi.signOut();
    } catch {
      // Best-effort: even if revoking server-side fails (e.g. already
      // offline), still clear local state below so the UI doesn't get
      // stuck "signed in" with a dead token.
    }
    setAccessToken(null);
    setSession(null);
  }, []);

  const value: AuthContextValue = {
    session,
    isBootstrapping,
    signIn,
    requestLoginOtp,
    verifyLoginOtp,
    requestSignupOtp,
    verifySignupOtp,
    completeSignup,
    requestPasswordResetOtp,
    verifyPasswordResetOtp,
    completePasswordReset,
    signOut,
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return ctx;
}
