import { API_BASE_URL, getAccessToken, silentRefresh } from './client';
import type { UploadRole } from '../types';

export interface UploadResult {
  threadId: string | null;
  extractedText: string;
  deduped: boolean;
  chunkCount: number;
}

interface UploadResponseBody {
  thread_id: string | null;
  extracted_text: string;
  deduped: boolean;
  chunk_count: number;
}

async function extractErrorMessage(response: Response): Promise<string> {
  const payload: unknown = await response.json().catch(() => null);
  return payload && typeof payload === 'object' && 'error' in payload && typeof payload.error === 'string'
    ? payload.error
    : `Request failed (${response.status})`;
}

// POST /uploads (backend/src/uploads/handlers.rs) — a real multipart call,
// not apiFetch (api/client.ts): apiFetch always sets
// Content-Type: application/json, which would break FormData's own
// multipart boundary, and this response is a single JSON body (not
// streamed), so api/memoryless.ts's raw-fetch SSE reader isn't the right
// model either — this sits in between, raw fetch for the multipart
// request, one response.json() for the reply.
export async function uploadFile(
  file: File,
  role: UploadRole,
  threadId: string | null,
  signal?: AbortSignal,
  isRetry = false,
): Promise<UploadResult> {
  const accessToken = getAccessToken();
  const body = new FormData();
  body.append('file', file, file.name);
  body.append('role', role);
  // Omitted entirely (not sent as an empty string) when null — matches
  // the backend's Option<Uuid> field, same convention already used by
  // api/memoryless.ts's streamMessage for thread_id.
  if (threadId) body.append('thread_id', threadId);

  const response = await fetch(`${API_BASE_URL}/uploads`, {
    method: 'POST',
    credentials: 'include',
    signal,
    headers: accessToken ? { Authorization: `Bearer ${accessToken}` } : {},
    body,
  });

  if (response.status === 401 && !isRetry && accessToken) {
    const refreshed = await silentRefresh();
    if (refreshed) {
      return uploadFile(file, role, threadId, signal, true);
    }
  }

  if (!response.ok) {
    throw new Error(await extractErrorMessage(response));
  }

  const payload = (await response.json()) as UploadResponseBody;
  return {
    threadId: payload.thread_id,
    extractedText: payload.extracted_text,
    deduped: payload.deduped,
    chunkCount: payload.chunk_count,
  };
}
