import { apiFetch, API_BASE_URL, getAccessToken, silentRefresh } from './client';
import type { ChatMessage } from '../types';

export interface JourneyStreamHandlers {
  onThreadId?: (threadId: string) => void; // only fires for startJourneyThread
  onConceptTitle?: (title: string) => void; // only fires for startJourneyThread
  onDelta: (text: string) => void;
  onError: (message: string) => void;
}

async function extractErrorMessage(response: Response): Promise<string> {
  const payload: unknown = await response.json().catch(() => null);
  return payload && typeof payload === 'object' && 'error' in payload && typeof payload.error === 'string'
    ? payload.error
    : `Request failed (${response.status})`;
}

// Shared by startJourneyThread/sendJourneyMessage — same raw-fetch SSE
// reader as api/memoryless.ts's streamMessage (apiFetch always calls
// response.json(), wrong for an SSE body; the browser's native
// EventSource can't send a POST body or an Authorization header).
// Cancellation: aborting `signal` closes the fetch, same as memoryless —
// the backend's spawned task detects the dropped receiver on its next
// send and stops pulling further chunks (deferred.md #2a mirrors
// memoryless/turn.rs's own cancellation shape exactly).
async function streamSse(
  path: string,
  body: unknown,
  handlers: JourneyStreamHandlers,
  signal: AbortSignal,
  isRetry = false,
): Promise<void> {
  const accessToken = getAccessToken();
  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}${path}`, {
      method: 'POST',
      credentials: 'include',
      signal,
      headers: {
        'Content-Type': 'application/json',
        ...(accessToken ? { Authorization: `Bearer ${accessToken}` } : {}),
      },
      body: JSON.stringify(body),
    });
  } catch {
    if (signal.aborted) return;
    handlers.onError('Could not reach the tutor. Check your connection and try again.');
    return;
  }

  if (response.status === 401 && !isRetry && accessToken) {
    const refreshed = await silentRefresh();
    if (refreshed) {
      return streamSse(path, body, handlers, signal, true);
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

        if (eventName === 'thread') handlers.onThreadId?.(data);
        else if (eventName === 'concept') handlers.onConceptTitle?.(data);
        else if (eventName === 'delta') handlers.onDelta(data);
        else if (eventName === 'error') handlers.onError(data);
        // "done" carries no payload this caller needs.

        boundary = buffer.indexOf('\n\n');
      }
    }
  } catch {
    if (signal.aborted) return;
    handlers.onError('Connection to the tutor was interrupted.');
  }
}

// POST /journeys/{journeyId}/start (backend/src/journeys/handlers.rs) —
// the tutor-initiated opening turn (deferred.md #2a). One-time call per
// journey — callers should GET first (getJourneyThread) and only call
// this when no thread exists yet.
export function startJourneyThread(
  journeyId: string,
  handlers: JourneyStreamHandlers,
  signal: AbortSignal,
  think = true,
): Promise<void> {
  return streamSse(`/journeys/${journeyId}/start`, { think }, handlers, signal);
}

// POST /journeys/{journeyId}/messages — every turn after the opening.
export function sendJourneyMessage(
  journeyId: string,
  message: string,
  handlers: JourneyStreamHandlers,
  signal: AbortSignal,
  think = true,
): Promise<void> {
  return streamSse(`/journeys/${journeyId}/messages`, { message, think }, handlers, signal);
}

interface JourneyMessageBody {
  role: 'user' | 'tutor';
  content: string;
  timestamp: string;
}
interface JourneyThreadBody {
  thread_id: string;
  current_concept_id: string;
  current_concept_title: string;
  messages: JourneyMessageBody[];
}

export interface JourneyThreadState {
  threadId: string;
  // #1/#2: real now (backend/src/journeys/turn.rs's ThreadHydration) —
  // previously only the title was ever exposed here, so nothing in the
  // frontend had a real concept id for the tutor-triggered exercise
  // path (deferred.md #94's own finding covers the OTHER path, the
  // Map, which still has none).
  currentConceptId: string;
  currentConceptTitle: string;
  messages: ChatMessage[];
}

// GET /journeys/{journeyId}/messages — hydration. `null` means no thread
// exists yet for this journey — the caller should call
// startJourneyThread instead of sendJourneyMessage.
export async function getJourneyThread(journeyId: string): Promise<JourneyThreadState | null> {
  const body = await apiFetch<JourneyThreadBody | null>(`/journeys/${journeyId}/messages`);
  if (!body) return null;
  return {
    threadId: body.thread_id,
    currentConceptId: body.current_concept_id,
    currentConceptTitle: body.current_concept_title,
    messages: body.messages.map((message, index) => ({
      id: `journey-${body.thread_id}-${index}`,
      role: message.role === 'user' ? 'student' : 'tutor',
      text: message.content,
      timestamp: message.timestamp,
    })),
  };
}
