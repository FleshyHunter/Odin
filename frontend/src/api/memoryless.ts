import { apiFetch, API_BASE_URL, getAccessToken, silentRefresh } from './client';
import type { ChatMessage } from '../types';

interface ConvertResponse {
  thread_id: string;
  converted: boolean;
}

// POST /memoryless/threads/:id/convert (backend/src/memoryless/
// handlers.rs::convert) — deferred.md #17: links this memoryless thread
// to a real, already-created journey (built via TrackModal's existing
// #4/#40 flow), committing its messages/audit_logs, flipping the entry
// concept to in_progress, and — if a prompt_upload was staged in this
// same conversation — its sources/chunks too. Called BEFORE
// onJourneyCreated in TrackModal, once a real journeyId exists.
export async function convertMemorylessThread(threadId: string, journeyId: string): Promise<void> {
  await apiFetch<ConvertResponse>(`/memoryless/threads/${threadId}/convert`, {
    method: 'POST',
    body: JSON.stringify({ journey_id: journeyId }),
  });
}

export interface MemorylessStreamHandlers {
  onThreadId: (threadId: string) => void;
  onDelta: (text: string) => void;
  onError: (message: string) => void;
}

// Matches backend/src/memoryless/staging.rs's StagedMessage — only the
// fields deferred.md #43's hydration path actually needs are declared.
interface StagedMessage {
  role: 'user' | 'tutor';
  content: string;
  timestamp: string;
}
interface StagedThreadResponse {
  thread_id: string;
  messages: StagedMessage[];
}

// GET /memoryless/threads/:id (backend/src/memoryless/handlers.rs::get_thread)
// — deferred.md #43: lets a direct /chat/:id visit for a real memoryless
// thread restore its message history, rather than only ever updating the
// URL with nothing behind it on reload. "user" maps to ChatMessage's
// 'student' role (StagedMessage's role string matches the messages
// table's own CHECK constraint, not this frontend's naming).
export async function getThread(threadId: string): Promise<ChatMessage[]> {
  const thread = await apiFetch<StagedThreadResponse>(`/memoryless/threads/${threadId}`);
  return thread.messages.map((message, index) => ({
    id: `staged-${threadId}-${index}`,
    role: message.role === 'user' ? 'student' : 'tutor',
    text: message.content,
    timestamp: message.timestamp,
  }));
}

interface SendMessageParams {
  threadId: string | null;
  message: string;
  think?: boolean;
}

async function extractErrorMessage(response: Response): Promise<string> {
  const payload: unknown = await response.json().catch(() => null);
  return payload && typeof payload === 'object' && 'error' in payload && typeof payload.error === 'string'
    ? payload.error
    : `Request failed (${response.status})`;
}

// POST /memoryless/messages (backend/src/memoryless/handlers.rs) returns
// a Server-Sent Events stream, not one JSON body — apiFetch (api/
// client.ts) always calls response.json(), so this is a separate raw-
// fetch helper that reads the body incrementally and parses SSE frames
// itself, rather than the browser's native EventSource (which can't send
// a POST body or an Authorization header).
//
// Cancellation: aborting `signal` closes the fetch, which is exactly
// what the backend's turn.rs relies on to detect a client disconnect
// (its spawned task's tx.send() starts failing, which IS the
// cancellation signal — see that file's own comment). No separate
// "stop" endpoint exists or is needed.
export async function streamMessage(
  params: SendMessageParams,
  handlers: MemorylessStreamHandlers,
  signal: AbortSignal,
  isRetry = false,
): Promise<void> {
  const accessToken = getAccessToken();
  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}/memoryless/messages`, {
      method: 'POST',
      credentials: 'include',
      signal,
      headers: {
        'Content-Type': 'application/json',
        ...(accessToken ? { Authorization: `Bearer ${accessToken}` } : {}),
      },
      body: JSON.stringify({
        thread_id: params.threadId,
        message: params.message,
        think: params.think ?? true,
      }),
    });
  } catch {
    if (signal.aborted) return; // user-initiated cancel, not a real failure
    handlers.onError('Could not reach the tutor. Check your connection and try again.');
    return;
  }

  // Same short-lived-access-token retry as apiFetch: a 401 mid-session
  // most likely means the 15-minute access token just expired.
  if (response.status === 401 && !isRetry && accessToken) {
    const refreshed = await silentRefresh();
    if (refreshed) {
      return streamMessage(params, handlers, signal, true);
    }
  }

  if (!response.ok || !response.body) {
    handlers.onError(await extractErrorMessage(response));
    return;
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });

      // SSE frames are separated by a blank line. Each frame carries an
      // "event: <name>" line and one-or-more "data:" lines — axum's
      // Event::data() splits embedded newlines across multiple "data:"
      // lines per the SSE spec, so every matching line is collected and
      // rejoined, not just the first.
      let boundary = buffer.indexOf('\n\n');
      while (boundary !== -1) {
        const frame = buffer.slice(0, boundary);
        buffer = buffer.slice(boundary + 2);

        let eventName: string | undefined;
        const dataLines: string[] = [];
        for (const line of frame.split('\n')) {
          if (line.startsWith('event:')) {
            eventName = line.slice('event:'.length).trim();
          } else if (line.startsWith('data:')) {
            dataLines.push(line.slice('data:'.length).replace(/^ /, ''));
          }
        }
        const data = dataLines.join('\n');

        if (eventName === 'thread') handlers.onThreadId(data);
        else if (eventName === 'delta') handlers.onDelta(data);
        // deferred.md #53: a real backend-signaled failure, distinct
        // from "delta" — previously a mid-generation failure was
        // invisible here, indistinguishable from an empty-but-
        // successful stream. Reuses the same onError callback as a
        // transport-level failure below; useMemorylessChat decides how
        // to treat it based on whether any delta already arrived.
        else if (eventName === 'error') handlers.onError(data);
        // "done" carries no payload this caller needs — the read loop's
        // own `done` (stream end) is what actually signals completion.

        boundary = buffer.indexOf('\n\n');
      }
    }
  } catch {
    if (signal.aborted) return; // user-initiated cancel, not a real failure
    handlers.onError('Connection to the tutor was interrupted.');
  }
}
