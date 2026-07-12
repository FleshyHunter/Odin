import type { AuthSession, User } from '../types';
import { simulateDelay } from './client';

const THIRTY_DAYS_MS = 30 * 24 * 60 * 60 * 1000;

function mockSession(email: string): AuthSession {
  const user: User = {
    userId: 'mock-user-1',
    displayName: 'Profile 1',
    email,
  };
  return {
    token: 'mock-jwt-token',
    user,
    expiresAt: new Date(Date.now() + THIRTY_DAYS_MS).toISOString(),
  };
}

// Real contract: POST /auth/login { email, password } -> AuthSession
export async function signIn(email: string, _password: string): Promise<AuthSession> {
  return simulateDelay(mockSession(email), 400);
}

// Real contract: POST /auth/signup { email, password } -> AuthSession
// (account starts email_verified=false per Auth section; NC6 — whether
// unverified accounts are usage-blocked or just reminded — is unresolved,
// so this stub does not model that distinction yet.)
export async function signUp(email: string, _password: string): Promise<AuthSession> {
  return simulateDelay(mockSession(email), 400);
}
