import { apiFetch } from './client';
import type { BackupQuestion, DiagnosticOutcome, JourneyStartResult, TrackIntake } from '../types';

interface StartResponseBody {
  diagnostic_id: string | null;
  question: string | null;
  exercise_type: string | null;
  choices: string[] | null;
  journey_id: string | null;
}

interface BackupQuestionBody {
  question: string;
  exercise_type: string;
  choices: string[] | null;
}

interface DiagnosticOutcomeBody {
  contradicted: boolean;
  backup_question: BackupQuestionBody | null;
  journey_id: string | null;
}

function toBackupQuestion(body: BackupQuestionBody | null): BackupQuestion | null {
  if (!body) return null;
  return { question: body.question, exerciseType: body.exercise_type, choices: body.choices };
}

function toOutcome(body: DiagnosticOutcomeBody): DiagnosticOutcome {
  return {
    contradicted: body.contradicted,
    backupQuestion: toBackupQuestion(body.backup_question),
    journeyId: body.journey_id,
  };
}

// POST /journeys/start (backend/src/journeys/handlers.rs) — deferred.md
// #4/#40. `intake` null means the skip path: level/goal are omitted
// entirely (matches the backend's Option<String> fields), not sent as
// empty strings — a real, distinct request shape, not a lesser version
// of the normal one.
export async function startJourney(topic: string, intake: TrackIntake | null): Promise<JourneyStartResult> {
  const body = await apiFetch<StartResponseBody>('/journeys/start', {
    method: 'POST',
    body: JSON.stringify({
      topic,
      level: intake?.level,
      goal: intake?.goal,
      background: intake?.background ?? undefined,
    }),
  });

  const diagnostic =
    body.diagnostic_id !== null
      ? {
          diagnosticId: body.diagnostic_id,
          question: body.question ?? '',
          exerciseType: body.exercise_type ?? '',
          choices: body.choices,
        }
      : null;

  return { diagnostic, journeyId: body.journey_id };
}

export async function respondToDiagnostic(diagnosticId: string, answer: string): Promise<DiagnosticOutcome> {
  const body = await apiFetch<DiagnosticOutcomeBody>(`/journeys/diagnostic/${diagnosticId}/respond`, {
    method: 'POST',
    body: JSON.stringify({ answer }),
  });
  return toOutcome(body);
}

export async function retryBackup(diagnosticId: string, answer: string): Promise<DiagnosticOutcome> {
  const body = await apiFetch<DiagnosticOutcomeBody>(`/journeys/diagnostic/${diagnosticId}/retry-backup`, {
    method: 'POST',
    body: JSON.stringify({ answer }),
  });
  return toOutcome(body);
}

export async function confirmDowngrade(diagnosticId: string): Promise<DiagnosticOutcome> {
  const body = await apiFetch<DiagnosticOutcomeBody>(`/journeys/diagnostic/${diagnosticId}/confirm-downgrade`, {
    method: 'POST',
  });
  return toOutcome(body);
}
