import { useId, useState, type FormEvent } from 'react';
import type { TrackGoal, TrackIntake, TrackLevel } from '../../../types';

const LEVELS: TrackLevel[] = ['Beginner', 'Intermediate', 'Advanced'];
const GOALS: TrackGoal[] = ['Exam prep', 'Project', 'General understanding'];

interface IntakeStepProps {
  onSubmit: (title: string, intake: TrackIntake | null) => void;
  onCancel: () => void;
  isSubmitting: boolean;
  error: string | null;
}

// PRD.md Onboarding Diagnostic Step 1's structured intake (deferred.md
// #40) — pure form UI, no network of its own. The orchestrator
// (TrackModal.tsx) owns the actual POST /journeys/start call and
// isSubmitting/error, since those also apply to later steps in the same
// wizard. "Skip diagnostic" mirrors PRD.md's trigger-phrase skip,
// translated into a real UI affordance since intake here is a form, not
// a chat message: it hides the three fields and submits with
// intake: null instead (backend/src/journeys really does treat that as
// a distinct request shape now — deferred.md #4's skip support — not
// just a frontend no-op).
export function IntakeStep({ onSubmit, onCancel, isSubmitting, error }: IntakeStepProps) {
  const [title, setTitle] = useState('');
  const [skipDiagnostic, setSkipDiagnostic] = useState(false);
  const [level, setLevel] = useState<TrackLevel | null>(null);
  const [goal, setGoal] = useState<TrackGoal | null>(null);
  const [background, setBackground] = useState('');
  const titleId = useId();
  const backgroundId = useId();

  const diagnosticComplete = skipDiagnostic || (level !== null && goal !== null);

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmedTitle = title.trim();
    if (!trimmedTitle || !diagnosticComplete || isSubmitting) return;

    const intake: TrackIntake | null =
      skipDiagnostic || !level || !goal ? null : { level, goal, background: background.trim() || null };
    onSubmit(trimmedTitle, intake);
  };

  return (
    <form className="modal-form" onSubmit={handleSubmit}>
      <label className="modal-field" htmlFor={titleId}>
        <span className="modal-label">What do you want to learn?</span>
        <input
          id={titleId}
          className="modal-input"
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          placeholder="Name your learning track"
          autoFocus
          required
        />
      </label>

      <p className="modal-hint">
        A track is a focused learning journey with its own conversation, roadmap, exercises, and progress.
      </p>

      {!skipDiagnostic && (
        <>
          <div className="modal-field">
            <span className="modal-label">What&apos;s your current level?</span>
            <div className="modal-option-list modal-option-row">
              {LEVELS.map((option) => (
                <button
                  key={option}
                  type="button"
                  className="modal-option"
                  aria-pressed={level === option}
                  onClick={() => setLevel(option)}
                >
                  {option}
                </button>
              ))}
            </div>
          </div>

          <div className="modal-field">
            <span className="modal-label">What&apos;s your goal?</span>
            <div className="modal-option-list modal-option-row">
              {GOALS.map((option) => (
                <button
                  key={option}
                  type="button"
                  className="modal-option"
                  aria-pressed={goal === option}
                  onClick={() => setGoal(option)}
                >
                  {option}
                </button>
              ))}
            </div>
          </div>

          <label className="modal-field" htmlFor={backgroundId}>
            <span className="modal-label">
              Anything else about your background?
              <span className="modal-optional">Optional</span>
            </span>
            <textarea
              id={backgroundId}
              className="modal-textarea"
              value={background}
              onChange={(event) => setBackground(event.target.value)}
              placeholder="Prior courses, relevant experience, anything that helps Odin place you accurately"
            />
          </label>
        </>
      )}

      <button type="button" className="modal-skip-link" onClick={() => setSkipDiagnostic((value) => !value)}>
        {skipDiagnostic ? 'Set my level and goal instead' : "Skip — I'll set my level later"}
      </button>

      {error && <p className="modal-error">{error}</p>}

      <div className="modal-actions">
        <button type="button" className="btn-secondary" onClick={onCancel} disabled={isSubmitting}>
          Cancel
        </button>
        <button type="submit" className="btn-primary" disabled={!title.trim() || !diagnosticComplete || isSubmitting}>
          {isSubmitting ? 'Starting...' : skipDiagnostic ? 'Create track' : 'Continue'}
        </button>
      </div>
    </form>
  );
}
