import { useEffect, useId, useState, type FormEvent } from 'react';
import { Button } from '../../ui/Button/Button';
import { Modal } from '../../ui/Modal/Modal';
import type { TrackGoal, TrackIntake, TrackLevel } from '../../../types';

const LEVELS: TrackLevel[] = ['Beginner', 'Intermediate', 'Advanced'];
const GOALS: TrackGoal[] = ['Exam prep', 'Project', 'General understanding'];

interface TrackModalProps {
  open: boolean;
  onClose: () => void;
  onCreate: (title: string, intake: TrackIntake | null) => Promise<unknown>;
}

// PRD.md Step 1's structured intake (deferred.md #40) — Level/Goal are
// required unless the student skips it. "Skip diagnostic" mirrors PRD.md's
// trigger-phrase skip, translated into a real UI affordance since intake
// here is a form, not a chat message: it hides the three fields and
// submits with intake: null instead.
export function TrackModal({ open, onClose, onCreate }: TrackModalProps) {
  const [title, setTitle] = useState('');
  const [skipDiagnostic, setSkipDiagnostic] = useState(false);
  const [level, setLevel] = useState<TrackLevel | null>(null);
  const [goal, setGoal] = useState<TrackGoal | null>(null);
  const [background, setBackground] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const titleId = useId();
  const backgroundId = useId();

  useEffect(() => {
    if (!open) return;
    setTitle('');
    setSkipDiagnostic(false);
    setLevel(null);
    setGoal(null);
    setBackground('');
    setError(null);
    setIsSubmitting(false);
  }, [open]);

  const diagnosticComplete = skipDiagnostic || (level !== null && goal !== null);

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmedTitle = title.trim();
    if (!trimmedTitle || !diagnosticComplete || isSubmitting) return;

    setIsSubmitting(true);
    setError(null);
    const intake: TrackIntake | null =
      skipDiagnostic || !level || !goal ? null : { level, goal, background: background.trim() || null };
    try {
      await onCreate(trimmedTitle, intake);
      onClose();
    } catch {
      setError('Track creation failed. Please try again.');
      setIsSubmitting(false);
    }
  };

  return (
    <Modal open={open} title="Create a track" onClose={onClose}>
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
          <Button variant="secondary" onClick={onClose} disabled={isSubmitting}>
            Cancel
          </Button>
          <Button type="submit" disabled={!title.trim() || !diagnosticComplete || isSubmitting}>
            {isSubmitting ? 'Creating...' : 'Create track'}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
