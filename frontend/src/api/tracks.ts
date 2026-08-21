import type { Track, TrackStatus } from '../types';
import { apiFetch } from './client';

// Real backend, backend/src/tracks/ — replaces the old fully-mocked
// version (in-memory array, lost on refresh). getMessages/sendMessage
// are gone: real chat for a Track always goes through useJourneyChat
// (api/journeyChat.ts) once its journeyId is set, never through here —
// they were dead weight even in the mock (ChatView.tsx only ever fell
// back to them for the pre-real-journey seed track, which no longer
// exists now that Tracks are real).
interface TrackBody {
  thread_id: string;
  title: string;
  subject_title: string;
  current_concept_title: string | null;
  status: string;
  is_pinned: boolean;
  project_id: string | null;
  last_active_at: string;
  journey_id: string;
}

function toTrack(body: TrackBody): Track {
  return {
    id: body.thread_id,
    title: body.title,
    subjectTitle: body.subject_title,
    currentConceptTitle: body.current_concept_title,
    status: body.status as TrackStatus,
    pinned: body.is_pinned,
    projectId: body.project_id,
    lastActiveAt: body.last_active_at,
    journeyId: body.journey_id,
  };
}

export async function listTracks(): Promise<Track[]> {
  const body = await apiFetch<TrackBody[]>('/tracks');
  return body.map(toTrack);
}

// Creates the real study_threads row AND generates the tutor's opening
// message in the same call (backend/src/journeys/turn.rs's
// create_journey_thread_sync) — slower than a plain insert (a real
// generation call), same tradeoff already accepted for the Onboarding
// Diagnostic step earlier in the same wizard this is called from.
export async function createTrackFromJourney(title: string, journeyId: string): Promise<Track> {
  const body = await apiFetch<TrackBody>('/tracks', {
    method: 'POST',
    body: JSON.stringify({ title, journey_id: journeyId }),
  });
  return toTrack(body);
}

// Deleting a track never affects mastery_bank — mastery is keyed on
// canonical_concept_id, not journey_id (ARCHITECTURE_LOCK.md, Rule 14).
// Soft delete backend-side (study_threads.deleted_at) — this call just
// stops it showing up in listTracks() from here on.
export async function deleteTrack(trackId: string): Promise<void> {
  await apiFetch<void>(`/tracks/${trackId}`, { method: 'DELETE' });
}

export async function togglePin(trackId: string): Promise<Track> {
  const body = await apiFetch<TrackBody>(`/tracks/${trackId}/pin`, { method: 'POST' });
  return toTrack(body);
}

export async function renameTrack(trackId: string, title: string): Promise<Track> {
  const body = await apiFetch<TrackBody>(`/tracks/${trackId}/rename`, {
    method: 'POST',
    body: JSON.stringify({ title }),
  });
  return toTrack(body);
}

// Same underlying operation for both TrackMenu's "Change project"
// (projectId set to a real id) and "Remove from project" (projectId set
// back to null) — deferred.md #41.
export async function setTrackProject(trackId: string, projectId: string | null): Promise<Track> {
  const body = await apiFetch<TrackBody>(`/tracks/${trackId}/project`, {
    method: 'POST',
    body: JSON.stringify({ project_id: projectId }),
  });
  return toTrack(body);
}
