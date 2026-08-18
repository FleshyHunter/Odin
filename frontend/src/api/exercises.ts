import type { Attempt, Difficulty, Exercise, MasteryStatus, SubmitAnswerResult } from '../types';
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
  return simulateDelay(
    { isCorrect: true, masteryScore: 0.7, expectedAnswer: 'x = 5', feedback: 'Nice — isolated the variable cleanly.' },
    400,
  );
}

// Real contract: GET /journeys/:journeyId/concepts/:conceptId/history -> Attempt[]
// Mirrors quiz_attempts, scoped to one concept — real once that table has
// a writer (deferred.md exercise-loop backend work, not built yet).
export async function getNodeHistory(_nodeId: string): Promise<Attempt[]> {
  return simulateDelay(
    [
      {
        id: 'a1',
        title: 'Isolating a variable',
        question: 'Solve for x: 2x + 3 = 7. Show the steps you took to isolate x on one side of the equation.',
        isCorrect: true,
        date: 'Aug 10',
        difficulty: 'basic',
      },
      {
        id: 'a2',
        title: 'Two-step equation',
        question: 'Solve for x: 5x − 1 = 14. Then check your answer by substituting it back into the original equation.',
        isCorrect: false,
        date: 'Aug 9',
        difficulty: 'intermediate',
      },
    ],
    300,
  );
}

const EXERCISE_BY_DIFFICULTY: Record<Difficulty, Pick<Exercise, 'title' | 'prompt' | 'answerPlaceholder'>> = {
  basic: {
    title: 'Isolating a variable',
    prompt: 'Solve for x: 2x + 3 = 7',
    answerPlaceholder: 'x = ?',
  },
  intermediate: {
    title: 'Two-step equation',
    prompt: 'Solve for x: 3x + 8 = 23',
    answerPlaceholder: 'x = ?',
  },
  advanced: {
    title: 'Equation with variables on both sides',
    prompt: 'Solve for x: 4x + 5 = 2x + 17',
    answerPlaceholder: 'x = ?',
  },
};

// Real contract: POST /journeys/:journeyId/concepts/:conceptId/exercise { difficulty } -> Exercise
// Instantiates a fresh templated exercise at the chosen difficulty — the
// same capability whether triggered live by the tutor (Now) or
// self-directed via the Map. No real "instantiate a stored template for
// serving" capability exists yet (only a private, validation-time-only
// helper in ai_service/app/acquisition/service.py) — separate backend work.
export async function startAttempt(_nodeId: string, difficulty: Difficulty): Promise<Exercise> {
  return simulateDelay(
    { id: `mock-${difficulty}-${Date.now()}`, conceptTitle: 'Linear Equations', difficulty, ...EXERCISE_BY_DIFFICULTY[difficulty] },
    400,
  );
}
