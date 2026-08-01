// Talks to the Rust backend ONLY. Per the locked AI Gateway Pattern
// (ARCHITECTURE_LOCK.md), the frontend never calls FastAPI directly —
// Rust is the sole boundary the browser talks to.
export const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? 'http://localhost:8080';

// Real backend wiring started with auth (api/auth.ts) — every other
// api/*.ts function is still a typed stub shaped like the real eventual
// response, using this helper to simulate network latency instead of a
// live fetch() that would just fail (tracks/projects/etc. haven't been
// wired yet). Swapping one in means replacing the simulateDelay(...)
// body with a call through apiFetch() below — the function signature
// and return type do not change.
export function simulateDelay<T>(value: T, ms = 500): Promise<T> {
  return new Promise((resolve) => setTimeout(() => resolve(value), ms));
}

// In-memory only, deliberately not persisted in localStorage — an XSS
// payload can't read what was never stored (the actual persistence
// mechanism is the httpOnly refresh_token cookie, which JS can't read
// either way; see hooks/useAuth.tsx's bootstrap effect + restoreSession
// in api/auth.ts for how a page reload gets a fresh access token back
// without the user ever seeing a login screen).
let accessToken: string | null = null;
export function setAccessToken(token: string | null): void {
  accessToken = token;
}
export function getAccessToken(): string | null {
  return accessToken;
}

// Calls POST /auth/refresh directly with a raw fetch rather than going
// through api/auth.ts's refresh() — that module imports apiFetch from
// here, so importing it back would be a circular import for the sake
// of one call. Coalesced via refreshInFlight so several near-
// simultaneous 401s (e.g. a few components' requests all landing right
// as the access token expires) trigger exactly one refresh, not one
// per request.
let refreshInFlight: Promise<boolean> | null = null;

// Exported so api/memoryless.ts's raw-fetch SSE call can reuse the same
// single-retry-on-401 behavior as apiFetch below, without duplicating
// the coalescing logic.
export async function silentRefresh(): Promise<boolean> {
  if (!refreshInFlight) {
    refreshInFlight = (async () => {
      try {
        const response = await fetch(`${API_BASE_URL}/auth/refresh`, {
          method: 'POST',
          credentials: 'include',
        });
        if (!response.ok) return false;
        const body: unknown = await response.json().catch(() => null);
        const token =
          body && typeof body === 'object' && 'access_token' in body && typeof body.access_token === 'string'
            ? body.access_token
            : null;
        if (!token) return false;
        setAccessToken(token);
        return true;
      } catch {
        return false;
      } finally {
        refreshInFlight = null;
      }
    })();
  }
  return refreshInFlight;
}

// Shared real-fetch helper. credentials: 'include' carries the httpOnly
// refresh_token cookie (Path=/auth/refresh, see backend/src/auth/
// handlers.rs) across the cross-origin dev setup (5173 -> 8080), which
// requires the backend's CORS layer to explicitly allow this origin
// with credentials (backend/src/main.rs) — a plain fetch() without this
// flag would just never receive/send that cookie at all.
//
// A 401 on a call that HAD an access token attached means it likely
// just expired mid-session (they're short-lived on purpose, 15 min) —
// try one silent refresh and retry exactly once before surfacing a real
// error. A 401 with no token attached (e.g. a wrong-password login,
// which never sends one) skips straight to the normal error path below
// instead of wasting a pointless refresh attempt. isRetry guards against
// ever looping more than once.
export async function apiFetch<T>(path: string, options: RequestInit = {}, isRetry = false): Promise<T> {
  const hadAccessToken = accessToken !== null;
  const response = await fetch(`${API_BASE_URL}${path}`, {
    ...options,
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
      ...(accessToken ? { Authorization: `Bearer ${accessToken}` } : {}),
      ...options.headers,
    },
  });

  if (response.status === 401 && !isRetry && hadAccessToken) {
    const refreshed = await silentRefresh();
    if (refreshed) {
      return apiFetch<T>(path, options, true);
    }
  }

  if (!response.ok) {
    const payload: unknown = await response.json().catch(() => null);
    const message =
      payload && typeof payload === 'object' && 'error' in payload && typeof payload.error === 'string'
        ? payload.error
        : `Request failed (${response.status})`;
    throw new Error(message);
  }
  // Several endpoints (e.g. /auth/signup/request-otp) respond with a
  // bare 200/204 and no body at all — response.json() throws on an
  // empty body ("Unexpected end of JSON input"), so check for actual
  // content first instead of only special-casing 204.
  const text = await response.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

// Thrown by a stub instead of a plain Error when it wants to simulate a
// REAL failure a caller should show gracefully (see ComposerNotice) —
// `tone` maps directly onto ComposerNoticeTone so a catch site doesn't
// need its own translation table. Once a stub is swapped for a real
// fetch(), a non-2xx response should be turned into this same shape
// (tone from the status code, message from the response body) so
// every caller's error handling keeps working unchanged.
export class ApiError extends Error {
  tone: 'warning' | 'error';

  constructor(tone: 'warning' | 'error', message: string) {
    super(message);
    this.tone = tone;
    this.name = 'ApiError';
  }
}
