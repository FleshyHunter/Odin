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
