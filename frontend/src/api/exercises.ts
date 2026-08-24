import { apiFetch, API_BASE_URL, getAccessToken, silentRefresh } from './client';
import type { Attempt, Difficulty, Exercise, MasteryStatus, SubmitAnswerResult } from '../types';

// #1/#2: real backend now (deferred.md #81's serve/submit/grade/
// mastery/advancement loop, wired to a real caller for the first time).
// Every function here is scoped to journeyId + conceptId, not trackId —
// the mock's own "Real contract" comments predated #81's actual build
// and described a different (track-scoped) shape that was never what
// got built. getCurrentExercise/getMasteryStatus/getNodeHistory/
// startAttempt/submitAnswer's mock comments are gone; see each
// function's own doc comment for what's actually real.

interface ServedExerciseBody {
  attempt_id: string;
  exercise_type: string;
  difficulty: string;
  rendered_question: string;
  rendered_choices: string[] | null;
}

// conceptTitle/title: Exercise has no real backend field for a short
// display name distinct from the question text (exercises only has
// template_body/question_template) — the caller-supplied conceptTitle
// fills both, same reasoning as MasteryStatus.conceptTitle.
function toExercise(body: ServedExerciseBody, conceptTitle: string): Exercise {
  return {
    id: body.attempt_id,
    conceptTitle,
    difficulty: body.difficulty as Difficulty,
    title: conceptTitle,
    prompt: body.rendered_question,
  };
}

// POST /journeys/{journeyId}/concepts/{conceptId}/exercise — instantiates
// a fresh templated exercise at the chosen difficulty. Same capability
// whether triggered live by the tutor (Now) or self-directed via the Map
// — though the Map path currently has no real conceptId to call this
// with at all (deferred.md #94), so only the tutor-triggered path is
// wired to anything real right now.
export async function startAttempt(journeyId: string, conceptId: string, difficulty: Difficulty, conceptTitle: string): Promise<Exercise> {
  const body = await apiFetch<ServedExerciseBody>(`/journeys/${journeyId}/concepts/${conceptId}/exercise`, {
    method: 'POST',
    body: JSON.stringify({ difficulty }),
  });
  return toExercise(body, conceptTitle);
}

// GET /journeys/{journeyId}/concepts/{conceptId}/exercise — no real
// endpoint exists for "is there already a current exercise" (serve_
// exercise is instantiate-fresh-on-demand, POST, not a thing to GET).
// Always resolves null, matching what the mock actually returned in
// every case anyway — no network call needed for an answer that's
// always the same.
export async function getCurrentExercise(_journeyId: string, _conceptId: string): Promise<Exercise | null> {
  return Promise.resolve(null);
}

interface MasteryStatusBody {
  mastery_score: number;
  is_complete: boolean;
  total_attempts: number;
}

// GET /journeys/{journeyId}/concepts/{conceptId}/mastery
export async function getMasteryStatus(journeyId: string, conceptId: string, conceptTitle: string): Promise<MasteryStatus | null> {
  const body = await apiFetch<MasteryStatusBody | null>(`/journeys/${journeyId}/concepts/${conceptId}/mastery`);
  if (!body) return null;
  return {
    conceptTitle,
    masteryScore: body.mastery_score,
    isComplete: body.is_complete,
    totalAttempts: body.total_attempts,
  };
}

interface AttemptBody {
  attempt_id: string;
  rendered_question: string;
  is_correct: boolean | null;
  difficulty_attempted: string | null;
  timestamp: string;
}

// GET /journeys/{journeyId}/concepts/{conceptId}/history. title: same
// "no real short-name field" gap as Exercise.title above — rendered_
// question fills both title and question, since there's nothing
// shorter to use.
export async function getNodeHistory(journeyId: string, conceptId: string): Promise<Attempt[]> {
  const body = await apiFetch<AttemptBody[]>(`/journeys/${journeyId}/concepts/${conceptId}/history`);
  return body.map((a) => ({
    id: a.attempt_id,
    title: a.rendered_question,
    question: a.rendered_question,
    isCorrect: a.is_correct ?? false,
    date: new Date(a.timestamp).toLocaleDateString('en-US', { month: 'short', day: 'numeric' }),
    difficulty: (a.difficulty_attempted ?? 'basic') as Difficulty,
  }));
}

export interface SubmitAnswerHandlers {
  onResult: (result: SubmitAnswerResult) => void;
  onDelta: (text: string) => void;
  onError: (message: string) => void;
}

interface ExerciseResultBody {
  is_correct: boolean;
  grade_score: number;
  new_mastery: number;
  expected_answer: string;
  feedback: string | null;
}

async function extractErrorMessage(response: Response): Promise<string> {
  const payload: unknown = await response.json().catch(() => null);
  return payload && typeof payload === 'object' && 'error' in payload && typeof payload.error === 'string'
    ? payload.error
    : `Request failed (${response.status})`;
}

// POST /journeys/{journeyId}/exercises/{attemptId}/submit — grades the
// answer (deterministic, Rule 2) and streams the tutor's reaction, same
// SSE shape as sendJourneyMessage (backend/src/journeyChat.ts's own
// streamSse). NOT a plain JSON POST despite the mock's old shape — the
// structured grading result arrives as its own `result` event, always
// first, before any `delta` (turn::submit_exercise_answer sends it
// before spawning the prose-streaming task, since grading/mastery are
// already fully computed and persisted by that point). The tutor's
// prose reaction is a real, persisted chat message like any other turn
// — this function only streams it via onDelta, the caller is
// responsible for folding those deltas into the same message list a
// normal turn would, not treating this as a side channel.
export async function submitAnswer(
  journeyId: string,
  attemptId: string,
  answer: string,
  handlers: SubmitAnswerHandlers,
  signal: AbortSignal,
  isRetry = false,
): Promise<void> {
  const accessToken = getAccessToken();
  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}/journeys/${journeyId}/exercises/${attemptId}/submit`, {
      method: 'POST',
      credentials: 'include',
      signal,
      headers: {
        'Content-Type': 'application/json',
        ...(accessToken ? { Authorization: `Bearer ${accessToken}` } : {}),
      },
      body: JSON.stringify({ answer }),
    });
  } catch {
    if (signal.aborted) return;
    handlers.onError('Could not reach the tutor. Check your connection and try again.');
    return;
  }

  if (response.status === 401 && !isRetry && accessToken) {
    const refreshed = await silentRefresh();
    if (refreshed) {
      return submitAnswer(journeyId, attemptId, answer, handlers, signal, true);
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

        if (eventName === 'result') {
          const body = JSON.parse(data) as ExerciseResultBody;
          handlers.onResult({
            isCorrect: body.is_correct,
            gradeScore: body.grade_score,
            newMastery: body.new_mastery,
            expectedAnswer: body.expected_answer,
            feedback: body.feedback ?? undefined,
          });
        } else if (eventName === 'delta') handlers.onDelta(data);
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
