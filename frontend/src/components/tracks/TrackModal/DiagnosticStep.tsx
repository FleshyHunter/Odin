import { useState, type FormEvent } from 'react';

interface DiagnosticStepProps {
  question: string;
  choices: string[] | null;
  onSubmit: (answer: string) => void;
  isSubmitting: boolean;
  error: string | null;
}

// Onboarding Diagnostic Steps 2-3 (PRD.md) — one question, answered
// either by picking a choice (multiple-choice, exerciseType "mcq") or
// typing a free-text answer (symbolic_math/numeric/etc — instantiate.rs
// never sends matrix data for a diagnostic, just plain text). Shared by
// both the primary question (from /journeys/start) and a backup question
// surfaced on contradiction — same shape either way, the orchestrator
// decides which endpoint the answer actually posts to.
export function DiagnosticStep({ question, choices, onSubmit, isSubmitting, error }: DiagnosticStepProps) {
  const [selectedChoice, setSelectedChoice] = useState<string | null>(null);
  const [freeTextAnswer, setFreeTextAnswer] = useState('');

  const answer = choices ? selectedChoice : freeTextAnswer.trim();

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!answer || isSubmitting) return;
    onSubmit(answer);
  };

  return (
    <form className="modal-form" onSubmit={handleSubmit}>
      <p className="exercise-q">{question}</p>

      {choices ? (
        <div className="modal-option-list">
          {choices.map((choice) => (
            <button
              key={choice}
              type="button"
              className="modal-option"
              aria-pressed={selectedChoice === choice}
              onClick={() => setSelectedChoice(choice)}
            >
              {choice}
            </button>
          ))}
        </div>
      ) : (
        <input
          className="modal-input"
          value={freeTextAnswer}
          onChange={(event) => setFreeTextAnswer(event.target.value)}
          placeholder="Your answer"
          autoFocus
        />
      )}

      {error && <p className="modal-error">{error}</p>}

      <div className="modal-actions">
        <button type="submit" className="btn-primary" disabled={!answer || isSubmitting}>
          {isSubmitting ? 'Checking...' : 'Submit'}
        </button>
      </div>
    </form>
  );
}
