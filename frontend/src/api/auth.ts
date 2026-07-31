import type { AuthSession, User } from '../types';
import { apiFetch, setAccessToken } from './client';

interface AuthResponseBody {
  access_token: string;
  user: { user_id: string; display_name: string; email: string };
}

function toSession({ access_token, user }: AuthResponseBody): AuthSession {
  setAccessToken(access_token);
  const mapped: User = { userId: user.user_id, displayName: user.display_name, email: user.email };
  return {
    token: access_token,
    user: mapped,
    // A rough client-side hint only (matches ACCESS_TOKEN_EXPIRY_MINUTES'
    // 15-minute default) — the real refresh token (httpOnly cookie) is
    // what actually governs session length; nothing here renews this yet.
    expiresAt: new Date(Date.now() + 15 * 60 * 1000).toISOString(),
  };
}

// POST /auth/signup/request-otp { email } -> 200, no body. Stages and
// emails a real 6-digit code (see backend/src/auth/otp.rs).
export async function requestSignupOtp(email: string): Promise<void> {
  await apiFetch<void>('/auth/signup/request-otp', {
    method: 'POST',
    body: JSON.stringify({ email }),
  });
}

// POST /auth/signup/verify-otp { email, code } -> 200, no body.
export async function verifySignupOtp(email: string, code: string): Promise<void> {
  await apiFetch<void>('/auth/signup/verify-otp', {
    method: 'POST',
    body: JSON.stringify({ email, code }),
  });
}

// POST /auth/signup/complete { email, display_name, password } ->
// AuthResponse. Only succeeds if verify-otp succeeded recently for this
// same email (backend/src/auth/handlers.rs: consume_signup_verified).
export async function completeSignup(email: string, displayName: string, password: string): Promise<AuthSession> {
  const result = await apiFetch<AuthResponseBody>('/auth/signup/complete', {
    method: 'POST',
    body: JSON.stringify({ email, display_name: displayName, password }),
  });
  return toSession(result);
}

// POST /auth/login/password { email, password } -> AuthResponse.
export async function signIn(email: string, password: string): Promise<AuthSession> {
  const result = await apiFetch<AuthResponseBody>('/auth/login/password', {
    method: 'POST',
    body: JSON.stringify({ email, password }),
  });
  return toSession(result);
}

// POST /auth/password-reset/request-otp { email } -> 200, no body.
// Always succeeds regardless of whether the account exists (anti-
// enumeration, backend/src/auth/handlers.rs) — only actually stages/
// sends a code when it does.
export async function requestPasswordResetOtp(email: string): Promise<void> {
  await apiFetch<void>('/auth/password-reset/request-otp', {
    method: 'POST',
    body: JSON.stringify({ email }),
  });
}

interface PasswordResetVerifyBody {
  display_name: string;
}

// POST /auth/password-reset/verify-otp { email, code } -> { display_name }.
// Returns the account's display name (safe to reveal here — a matching
// code could only exist for a real account) so the final step can show
// "Hi <name>" instead of a blank form.
export async function verifyPasswordResetOtp(email: string, code: string): Promise<string> {
  const result = await apiFetch<PasswordResetVerifyBody>('/auth/password-reset/verify-otp', {
    method: 'POST',
    body: JSON.stringify({ email, code }),
  });
  return result.display_name;
}

// POST /auth/password-reset/complete { email, new_password } ->
// AuthResponse. Auto-logs in on success, same as completeSignup.
export async function completePasswordReset(email: string, newPassword: string): Promise<AuthSession> {
  const result = await apiFetch<AuthResponseBody>('/auth/password-reset/complete', {
    method: 'POST',
    body: JSON.stringify({ email, new_password: newPassword }),
  });
  return toSession(result);
}

// POST /auth/login/request-otp { email } -> 200, no body. Same anti-
// enumeration shape as signup/password-reset's request-otp — always
// succeeds, only actually stages/sends a code for a real account.
export async function requestLoginOtp(email: string): Promise<void> {
  await apiFetch<void>('/auth/login/request-otp', {
    method: 'POST',
    body: JSON.stringify({ email }),
  });
}

// POST /auth/login/verify-otp { email, code } -> AuthResponse. Unlike
// signup/password-reset's verify-otp, login's already returns full
// tokens directly (backend/src/auth/handlers.rs's login_verify_otp
// calls respond_with_tokens) — logging in needs no profile completion,
// so there's no separate "complete" step.
export async function verifyLoginOtp(email: string, code: string): Promise<AuthSession> {
  const result = await apiFetch<AuthResponseBody>('/auth/login/verify-otp', {
    method: 'POST',
    body: JSON.stringify({ email, code }),
  });
  return toSession(result);
}

interface MeResponseBody {
  user_id: string;
  display_name: string;
  email: string;
}

// GET /auth/me -> { user_id, display_name, email }. Requires an access
// token already be set (Authorization header, see client.ts) — callers
// use this right after refresh() below, never on its own.
async function me(): Promise<MeResponseBody> {
  return apiFetch<MeResponseBody>('/auth/me');
}

interface RefreshResponseBody {
  access_token: string;
}

// POST /auth/refresh -> { access_token }. No body needed — entirely
// driven by the httpOnly refresh_token cookie (credentials: 'include',
// handled by apiFetch). Backend rotates the refresh token AND resets
// its 30-day window on every call (sliding expiry + rotation,
// backend/src/auth/handlers.rs) — this is what makes normal, regular
// use never actually hit the 30-day wall.
async function refresh(): Promise<string> {
  const result = await apiFetch<RefreshResponseBody>('/auth/refresh', { method: 'POST' });
  setAccessToken(result.access_token);
  return result.access_token;
}

// The actual "am I still logged in" check — run once on app load (see
// hooks/useAuth.tsx's bootstrap effect). Combines refresh (mints a
// fresh access token off the refresh cookie) + me (fetches the profile
// to display) into one AuthSession. Throws if there's no valid cookie
// (never logged in, expired, or revoked) — the normal, expected case
// for a first-time or logged-out visitor, not a real error.
export async function restoreSession(): Promise<AuthSession> {
  const accessToken = await refresh();
  const user = await me();
  return {
    token: accessToken,
    user: { userId: user.user_id, displayName: user.display_name, email: user.email },
    expiresAt: new Date(Date.now() + 15 * 60 * 1000).toISOString(),
  };
}

// DELETE /auth/refresh -> 204. Revokes only THIS device's refresh
// token (backend/src/auth/handlers.rs's logout) — other signed-in
// devices/sessions are untouched, matching the "log out of this device
// only" behavior the sidebar's profile menu describes.
export async function signOut(): Promise<void> {
  await apiFetch<void>('/auth/refresh', { method: 'DELETE' });
}
