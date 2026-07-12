import type { Exercise, MasteryStatus, SubmitAnswerResult } from '../types';
import { simulateDelay } from './client';

// Real contract: GET /tracks/:trackId/exercise/current -> Exercise | null
// null until the teaching loop (Flow 2) actually generates one for this
// track's current concept — no starter exercise exists yet.
export async function getCurrentExercise(_trackId: string): Promise<Exercise | null> {
  return simulateDelay(null);
}

// Real contract: GET /tracks/:trackId/mastery -> MasteryStatus | null
// null until mastery_bank actually has a row for this track's concept.
export async function getMasteryStatus(_trackId: string): Promise<MasteryStatus | null> {
  return simulateDelay(null);
}

// Real contract: POST /exercises/:exerciseId/submit { answer } -> SubmitAnswerResult
// Grading is always deterministic server-side (Rule 2) — no LLM grading,
// this stub never actually evaluates the answer text.
export async function submitAnswer(_exerciseId: string, _answer: string): Promise<SubmitAnswerResult> {
  return simulateDelay({ isCorrect: true, masteryScore: 0.7 }, 400);
}
