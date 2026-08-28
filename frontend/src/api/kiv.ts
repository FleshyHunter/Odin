import { apiFetch } from './client';

// POST /journeys/{journeyId}/concepts/{conceptId}/skip
// (backend/src/journeys/handlers.rs) — deferred.md #38. The one real
// trigger for kiv_flagged_at, unified across both of PRD.md's locked KIV
// conditions ("moves on from a failed advanced question" — ExerciseCard's
// "Move on for now" — or "skips a foundation_gap concept" — NodeDetail's
// "Skip"). No response body — the roadmap refetch that follows is what
// picks the change up.
export async function skipConcept(journeyId: string, conceptId: string): Promise<void> {
  await apiFetch<void>(`/journeys/${journeyId}/concepts/${conceptId}/skip`, { method: 'POST' });
}
