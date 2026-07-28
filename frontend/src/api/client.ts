// Talks to the Rust backend ONLY. Per the locked AI Gateway Pattern
// (ARCHITECTURE_LOCK.md), the frontend never calls FastAPI directly —
// Rust is the sole boundary the browser talks to.
export const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? 'http://localhost:8080';

// The Rust backend doesn't exist yet (frontend comes before backend
// connection work per Implementation Milestones). Every api/*.ts function
// is a typed stub shaped like the real eventual response, using this
// helper to simulate network latency instead of a live fetch() that
// would just fail. Swapping in the real call later means replacing the
// simulateDelay(...) body with an actual `fetch(`${API_BASE_URL}...`)`
// — the function signature and return type do not change.
export function simulateDelay<T>(value: T, ms = 500): Promise<T> {
  return new Promise((resolve) => setTimeout(() => resolve(value), ms));
}

// Thrown by a stub instead of a plain Error when it wants to simulate a
// REAL failure a caller should show gracefully (see ComposerNotice) —
// `tone` maps directly onto ComposerNoticeTone so a catch site doesn't
// need its own translation table. Once a stub is swapped for a real
// fetch(), a non-2xx response should be turned into this same shape
// (tone from the status code, message from the response body) so
// every caller's error handling keeps working unchanged.
export class ApiError extends Error {
  constructor(
    public tone: 'warning' | 'error',
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}
