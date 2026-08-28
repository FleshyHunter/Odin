import { useEffect, useState } from 'react';
import type { Exercise, SubmitAnswerResult } from '../../types';
import { Button } from '../ui/Button/Button';
import * as contentFlagsApi from '../../api/contentFlags';
import './exerciseCard.css';

interface ExerciseCardProps {
  exercise: Exercise;
  onSubmit?: (answer: string) => Promise<SubmitAnswerResult | undefined>;
  // deferred.md #38 — PRD.md's locked KIV trigger: "moves on from a
  // failed advanced question." Only ever offered for that exact case
  // (see the render below) — never a generic "give up" button.
  onMoveOn?: () => void;
}

function capitalize(word: string): string {
  return word.charAt(0).toUpperCase() + word.slice(1);
}

const FLAG_ICON = (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8}>
    <path d="M5 3v18" />
    <path d="M5 4h11l-2.5 4L16 12H5" />
  </svg>
);

export function ExerciseCard({ exercise, onSubmit, onMoveOn }: ExerciseCardProps) {
  const [answer, setAnswer] = useState('');
  const [isFlagging, setIsFlagging] = useState(false);
  const [flagState, setFlagState] = useState<'idle' | 'flagged' | 'error'>('idle');
  const [view, setView] = useState<'question' | 'answer'>('question');
  const [result, setResult] = useState<SubmitAnswerResult | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [movedOn, setMovedOn] = useState(false);

  // ExerciseCard isn't remounted between exercises (ActivePanel renders
  // it without a key), so local state has to reset itself whenever the
  // exercise it's describing actually changes — otherwise a "Flagged"
  // confirmation, or a PREVIOUS exercise's answer reveal, would bleed
  // into the next one.
  useEffect(() => {
    setFlagState('idle');
    setView('question');
    setResult(null);
    setAnswer('');
    setMovedOn(false);
  }, [exercise.id]);

  const handleMoveOn = () => {
    onMoveOn?.();
    setMovedOn(true);
  };

  const handleSubmit = async () => {
    if (!answer.trim() || isSubmitting) return;
    setIsSubmitting(true);
    try {
      const outcome = await onSubmit?.(answer.trim());
      if (outcome) setResult(outcome);
    } finally {
      setIsSubmitting(false);
    }
  };

  // deferred.md #49/Rule 32 — the one, manual "flag as wrong" correction
  // mechanism. Native prompt() for the reason, matching this app's
  // existing convention for small one-off inputs (TrackMenu's delete
  // confirmation uses the same native-dialog approach rather than a
  // custom modal).
  const handleFlag = async () => {
    const reason = window.prompt('What is wrong with this exercise?');
    if (!reason || !reason.trim()) return;
    setIsFlagging(true);
    try {
      await contentFlagsApi.createFlag({ exerciseId: exercise.id, reason: reason.trim() });
      setFlagState('flagged');
    } catch {
      setFlagState('error');
    } finally {
      setIsFlagging(false);
    }
  };

  return (
    <div className="exercise-card">
      <div className="exercise-card-top">
        <span className="difficulty-tag">{capitalize(exercise.difficulty)}</span>
        {flagState === 'flagged' ? (
          <span className="flag-status">Flagged for review</span>
        ) : (
          <button
            type="button"
            className="flag-button"
            onClick={handleFlag}
            disabled={isFlagging}
            aria-label="Flag this exercise as wrong"
          >
            {FLAG_ICON}
            {flagState === 'error' ? 'Could not flag — retry' : 'Flag'}
          </button>
        )}
      </div>
      <div className="qa-toggle">
        <button
          type="button"
          className={view === 'question' ? 'qa-btn qa-active' : 'qa-btn'}
          onClick={() => setView('question')}
        >
          Question
        </button>
        <button
          type="button"
          className={view === 'answer' ? 'qa-btn qa-active' : 'qa-btn'}
          onClick={() => result && setView('answer')}
          disabled={!result}
          aria-label={result ? 'View answer' : 'Answer locked until you submit'}
        >
          Answer
        </button>
      </div>

      {view === 'question' ? (
        <>
          <p className="exercise-q">{exercise.prompt}</p>

          <input
            className="answer-field"
            type="text"
            placeholder={exercise.answerPlaceholder ?? ''}
            value={answer}
            onChange={(event) => setAnswer(event.target.value)}
          />
          <Button onClick={handleSubmit} disabled={isSubmitting}>
            {isSubmitting ? 'Checking…' : 'Submit'}
          </Button>
        </>
      ) : (
        result && (
          <div className={result.isCorrect ? 'answer-reveal' : 'answer-reveal incorrect'}>
            <div className="answer-reveal-label">{result.isCorrect ? 'Correct' : 'Not quite'}</div>
            {result.expectedAnswer && <div>{result.expectedAnswer}</div>}
            {result.feedback && <div>{result.feedback}</div>}
            {/* deferred.md #38 — PRD.md's locked KIV trigger, exact case:
                "moves on from a failed advanced question." */}
            {!result.isCorrect && exercise.difficulty === 'advanced' && onMoveOn && (
              <div className="move-on-row">
                {movedOn ? (
                  <span className="move-on-confirm">Flagged for review — you'll find it in the KIV tab.</span>
                ) : (
                  <button type="button" className="move-on-btn" onClick={handleMoveOn}>
                    Move on for now
                  </button>
                )}
              </div>
            )}
          </div>
        )
      )}
    </div>
  );
}
