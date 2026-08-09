import { useEffect, useState } from 'react';
import { Modal } from '../../ui/Modal/Modal';
import { IntakeStep } from './IntakeStep';
import { DiagnosticStep } from './DiagnosticStep';
import { ContradictionStep } from './ContradictionStep';
import * as journeysApi from '../../../api/journeys';
import * as memorylessApi from '../../../api/memoryless';
import type { BackupQuestion, DiagnosticOutcome, DiagnosticQuestion, TrackIntake } from '../../../types';

type Step = 'intake' | 'diagnostic' | 'contradiction' | 'backup';

interface TrackModalProps {
  open: boolean;
  onClose: () => void;
  onJourneyCreated: (title: string, journeyId: string) => Promise<unknown>;
  // deferred.md #8/#17: set when this modal was opened via the memoryless
  // nudge (or a future manual "add to a track" trigger, #74) — the
  // student names the new track same as any fresh one (no reliable way
  // to infer a topic from chat content without another LLM call), but on
  // reaching a real journeyId, that memoryless thread is converted into
  // it (real messages/audit_logs + any staged prompt_upload committed)
  // BEFORE onJourneyCreated fires.
  initialThreadId?: string | null;
}

const STEP_TITLES: Record<Step, string> = {
  intake: 'Create a track',
  diagnostic: 'Quick check',
  contradiction: 'One moment',
  backup: 'Backup question',
};

// Onboarding Diagnostic orchestration (PRD.md Steps 1-4, deferred.md
// #4/#40) — a real multi-step wizard, matching this codebase's existing
// pattern (components/auth/signup/SignupFlow.tsx: an orchestrator owning
// step state + the real backend calls, dumb per-step form components).
// Intake -> POST /journeys/start. Either that returns a journey_id
// immediately (skip path) or a question to answer (Diagnostic step).
// A wrong answer moves to Contradiction (PRD.md's required disclosure —
// "tell the student directly, don't silently reassign"), which offers
// Confirm (-> confirm-downgrade) or, if a backup exists, Try backup
// question (-> Backup step, same DiagnosticStep component, answered via
// retry-backup instead). Any outcome carrying a real journey_id finishes
// the flow.
export function TrackModal({ open, onClose, onJourneyCreated, initialThreadId = null }: TrackModalProps) {
  const [step, setStep] = useState<Step>('intake');
  const [title, setTitle] = useState('');
  const [level, setLevel] = useState<string | null>(null);
  const [diagnosticId, setDiagnosticId] = useState<string | null>(null);
  const [question, setQuestion] = useState<DiagnosticQuestion | BackupQuestion | null>(null);
  const [pendingBackup, setPendingBackup] = useState<BackupQuestion | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setStep('intake');
    setTitle('');
    setLevel(null);
    setDiagnosticId(null);
    setQuestion(null);
    setPendingBackup(null);
    setIsSubmitting(false);
    setError(null);
  }, [open]);

  const finish = async (finalTitle: string, journeyId: string) => {
    if (initialThreadId) {
      await memorylessApi.convertMemorylessThread(initialThreadId, journeyId);
    }
    await onJourneyCreated(finalTitle, journeyId);
    onClose();
  };

  const handleOutcome = async (finalTitle: string, outcome: DiagnosticOutcome) => {
    if (outcome.journeyId) {
      await finish(finalTitle, outcome.journeyId);
      return;
    }
    // Contradicted, awaiting a decision — PRD.md Step 3's disclosure.
    setPendingBackup(outcome.backupQuestion);
    setStep('contradiction');
  };

  const handleIntakeSubmit = async (submittedTitle: string, intake: TrackIntake | null) => {
    setIsSubmitting(true);
    setError(null);
    setTitle(submittedTitle);
    setLevel(intake?.level ?? null);
    try {
      const result = await journeysApi.startJourney(submittedTitle, intake);
      if (result.journeyId) {
        await finish(submittedTitle, result.journeyId);
        return;
      }
      if (result.diagnostic) {
        setDiagnosticId(result.diagnostic.diagnosticId);
        setQuestion(result.diagnostic);
        setStep('diagnostic');
      }
    } catch {
      setError('Could not start this track. Please try again.');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDiagnosticSubmit = async (answer: string) => {
    if (!diagnosticId) return;
    setIsSubmitting(true);
    setError(null);
    try {
      const outcome = await journeysApi.respondToDiagnostic(diagnosticId, answer);
      await handleOutcome(title, outcome);
    } catch {
      setError('Could not check that answer. Please try again.');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleBackupSubmit = async (answer: string) => {
    if (!diagnosticId) return;
    setIsSubmitting(true);
    setError(null);
    try {
      const outcome = await journeysApi.retryBackup(diagnosticId, answer);
      await handleOutcome(title, outcome);
    } catch {
      setError('Could not check that answer. Please try again.');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleConfirmDowngrade = async () => {
    if (!diagnosticId) return;
    setIsSubmitting(true);
    setError(null);
    try {
      const outcome = await journeysApi.confirmDowngrade(diagnosticId);
      await handleOutcome(title, outcome);
    } catch {
      setError('Could not confirm that. Please try again.');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleTryBackup = () => {
    if (!pendingBackup) return;
    setQuestion(pendingBackup);
    setStep('backup');
  };

  return (
    <Modal open={open} title={STEP_TITLES[step]} onClose={onClose}>
      {step === 'intake' && <IntakeStep onSubmit={handleIntakeSubmit} onCancel={onClose} isSubmitting={isSubmitting} error={error} />}

      {(step === 'diagnostic' || step === 'backup') && question && (
        <DiagnosticStep
          question={question.question}
          choices={question.choices}
          onSubmit={step === 'diagnostic' ? handleDiagnosticSubmit : handleBackupSubmit}
          isSubmitting={isSubmitting}
          error={error}
        />
      )}

      {step === 'contradiction' && (
        <ContradictionStep
          level={level ?? ''}
          hasBackup={pendingBackup !== null}
          onConfirmDowngrade={handleConfirmDowngrade}
          onTryBackup={handleTryBackup}
          isSubmitting={isSubmitting}
          error={error}
        />
      )}
    </Modal>
  );
}
