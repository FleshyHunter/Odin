import type { ChatMessage, Track } from '../types';
import { simulateDelay } from './client';

// In-memory only — stands in for a real backend/DB row set. Normally
// starts empty (true empty state — no tracks until the user creates one);
// one demo entry is seeded below at the user's explicit request, to have
// something on screen while designing/reviewing the Tracks list page.
// Remove this entry to go back to a true empty start. Lost on page
// refresh either way; that's expected for a stub, not a bug.
const tracks: Track[] = [
  {
    id: 'track-seed-1',
    title: 'Linear Algebra',
    subjectTitle: 'Linear Algebra',
    currentConceptTitle: 'Eigenvalues & Eigenvectors',
    status: 'active',
    pinned: false,
    projectId: null,
    lastActiveAt: new Date().toISOString(),
  },
];

// Real contract: GET /tracks -> Track[]
export async function listTracks(): Promise<Track[]> {
  return simulateDelay([...tracks]);
}

// Real contract: POST /tracks { title } -> Track
// subjectTitle mirrors title for now — there's no real subject/DAG
// creation flow yet (Flow 4, ARCHITECTURE_LOCK.md), so a fresh track has
// no canonical subject or concept assigned until that exists.
export async function createTrack(title: string): Promise<Track> {
  const track: Track = {
    id: `track-${Date.now()}`,
    title,
    subjectTitle: title,
    currentConceptTitle: null,
    status: 'active',
    pinned: false,
    projectId: null,
    lastActiveAt: new Date().toISOString(),
  };
  tracks.push(track);
  return simulateDelay(track, 300);
}

// Real contract: GET /tracks/:trackId/messages -> ChatMessage[]
// No track starts with any conversation history — always empty until
// real messages are sent.
export async function getMessages(_trackId: string): Promise<ChatMessage[]> {
  return simulateDelay([]);
}

// Real contract: POST /tracks/:trackId/messages { text } -> ChatMessage (tutor reply)
// study_threads are written to PostgreSQL only on first message (Rule 11) —
// this stub doesn't model that distinction, it's a backend-side concern.
export async function sendMessage(_trackId: string, text: string): Promise<ChatMessage> {
  return simulateDelay(
    {
      id: `tutor-${Date.now()}`,
      role: 'tutor',
      text: `(mock tutor reply to: "${text}")`,
      timestamp: new Date().toISOString(),
    },
    600,
  );
}

// Real contract: DELETE /tracks/:trackId
// Deleting a track never affects mastery_bank — mastery is keyed on
// canonical_concept_id, not journey_id (ARCHITECTURE_LOCK.md, Rule 14).
export async function deleteTrack(trackId: string): Promise<void> {
  const index = tracks.findIndex((t) => t.id === trackId);
  if (index !== -1) tracks.splice(index, 1);
  return simulateDelay(undefined, 300);
}

// Real contract: PATCH /tracks/:trackId { pinned } -> Track
export async function togglePin(trackId: string): Promise<Track> {
  const track = tracks.find((t) => t.id === trackId);
  if (!track) throw new Error(`Track ${trackId} not found`);
  track.pinned = !track.pinned;
  return simulateDelay(track, 200);
}
