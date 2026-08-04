import { apiFetch } from './client';

interface CreateFlagRequest {
  exerciseId?: string;
  sourceId?: string;
  chunkId?: string;
  reason: string;
}

interface FlagResponse {
  flag_id: string;
}

// POST /content_flags (backend/src/content_flags/handlers.rs) — Rule
// 32's manual "flag as wrong" action, the SOLE correction mechanism for
// bad/conflicting content. Real endpoint, already built and verified
// (deferred.md #33/#44); this is its first frontend caller (deferred.md
// #49). Exactly one of exerciseId/sourceId/chunkId must be set, matching
// the backend's own CHECK-constraint-mirroring validation.
export async function createFlag(request: CreateFlagRequest): Promise<FlagResponse> {
  return apiFetch<FlagResponse>('/content_flags', {
    method: 'POST',
    body: JSON.stringify({
      exercise_id: request.exerciseId ?? null,
      source_id: request.sourceId ?? null,
      chunk_id: request.chunkId ?? null,
      reason: request.reason,
    }),
  });
}
