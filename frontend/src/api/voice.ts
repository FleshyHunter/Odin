import { API_BASE_URL, getAccessToken, silentRefresh } from './client';

interface TranscribeResponseBody {
  text: string;
}

async function extractErrorMessage(response: Response): Promise<string> {
  const payload: unknown = await response.json().catch(() => null);
  return payload && typeof payload === 'object' && 'error' in payload && typeof payload.error === 'string'
    ? payload.error
    : `Request failed (${response.status})`;
}

// POST /voice/transcribe (backend/src/voice/handlers.rs, deferred.md #80)
// — real multipart call, same shape/reasoning as api/uploads.ts's own
// uploadFile: not apiFetch (forces Content-Type: application/json,
// which would break FormData's multipart boundary), a single JSON body
// reply (not streamed), so a raw fetch + one response.json() sits in
// between apiFetch and memoryless.ts's SSE reader.
export async function transcribeAudio(
  audioBlob: Blob,
  filename: string,
  signal?: AbortSignal,
  isRetry = false,
): Promise<string> {
  const accessToken = getAccessToken();
  const body = new FormData();
  body.append('file', audioBlob, filename);

  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}/voice/transcribe`, {
      method: 'POST',
      credentials: 'include',
      signal,
      headers: accessToken ? { Authorization: `Bearer ${accessToken}` } : {},
      body,
    });
  } catch (err) {
    if (signal?.aborted) throw err; // user-initiated cancel, not a real failure
    throw new Error('Could not reach the tutor. Check your connection and try again.');
  }

  if (response.status === 401 && !isRetry && accessToken) {
    const refreshed = await silentRefresh();
    if (refreshed) {
      return transcribeAudio(audioBlob, filename, signal, true);
    }
  }

  if (!response.ok) {
    throw new Error(await extractErrorMessage(response));
  }

  const payload = (await response.json()) as TranscribeResponseBody;
  return payload.text;
}

export interface VoiceStreamHandlers {
  onSessionId: (id: string) => void;
  // Always the FULL re-decoded transcript-so-far, never an increment —
  // Whisper re-transcribes the entire growing buffer on every chunk, so
  // the caller must replace its displayed text with this value, not
  // append to it. See useVoiceInput.ts's own comment.
  onPartial: (text: string) => void;
  onError: (message: string) => void;
}

// POST /voice/transcribe/stream (backend/src/voice/handlers.rs::stream_start)
// — opened once per recording, before any audio is uploaded. Same
// hand-rolled SSE-over-fetch reader shape as memoryless.ts's
// streamMessage (see its own comment for why not native EventSource).
// No request body: this call only opens the channel and yields a
// session id; the actual audio goes over separate
// uploadTranscriptionChunk() calls below, correlated by that id.
export async function streamTranscription(
  handlers: VoiceStreamHandlers,
  signal: AbortSignal,
  isRetry = false,
): Promise<void> {
  const accessToken = getAccessToken();
  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}/voice/transcribe/stream`, {
      method: 'POST',
      credentials: 'include',
      signal,
      headers: accessToken ? { Authorization: `Bearer ${accessToken}` } : {},
    });
  } catch {
    if (signal.aborted) return; // user-initiated cancel, not a real failure
    handlers.onError('Could not reach the transcription service.');
    return;
  }

  if (response.status === 401 && !isRetry && accessToken) {
    const refreshed = await silentRefresh();
    if (refreshed) {
      return streamTranscription(handlers, signal, true);
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

        if (eventName === 'session') handlers.onSessionId(data);
        else if (eventName === 'partial') handlers.onPartial(data);
        else if (eventName === 'error') handlers.onError(data);
        // "done" carries no payload this caller needs.

        boundary = buffer.indexOf('\n\n');
      }
    }
  } catch {
    if (signal.aborted) return; // user-initiated cancel, not a real failure
    handlers.onError('Connection to the transcription service was interrupted.');
  }
}

// POST /voice/transcribe/chunk (backend/src/voice/handlers.rs::stream_chunk)
// — fired every few seconds with the FULL growing audio buffer
// re-uploaded each time, not just the new bytes (see useVoiceInput.ts's
// own comment on why). The response body is intentionally ignored —
// the real result arrives over the streamTranscription SSE channel
// above, not this response. Deliberately no error surfacing here: a
// failed chunk upload just means one missed partial, not worth
// interrupting the recording over.
export async function uploadTranscriptionChunk(
  audioBlob: Blob,
  filename: string,
  sessionId: string,
  signal?: AbortSignal,
): Promise<void> {
  const accessToken = getAccessToken();
  const body = new FormData();
  body.append('file', audioBlob, filename);
  body.append('session_id', sessionId);

  await fetch(`${API_BASE_URL}/voice/transcribe/chunk`, {
    method: 'POST',
    credentials: 'include',
    signal,
    headers: accessToken ? { Authorization: `Bearer ${accessToken}` } : {},
    body,
  });
}
