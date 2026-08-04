import { useEffect, useState } from 'react';
import type { Exercise } from '../../types';
import { Button } from '../ui/Button/Button';
import { MatrixRenderer } from './MatrixRenderer';
import * as contentFlagsApi from '../../api/contentFlags';

interface ExerciseCardProps {
  exercise: Exercise;
  onSubmit?: (answer: string) => void;
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

export function ExerciseCard({ exercise, onSubmit }: ExerciseCardProps) {
  const [answer, setAnswer] = useState('');
  const [isFlagging, setIsFlagging] = useState(false);
  const [flagState, setFlagState] = useState<'idle' | 'flagged' | 'error'>('idle');

  // ExerciseCard isn't remounted between exercises (ActivePanel renders
  // it without a key), so this local flag state has to reset itself
  // whenever the exercise it's describing actually changes — otherwise
  // a "Flagged" confirmation from a previous exercise would bleed into
  // the next one.
  useEffect(() => {
    setFlagState('idle');
  }, [exercise.id]);

  const handleSubmit = () => {
    if (!answer.trim()) return;
    onSubmit?.(answer.trim());
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
      <p className="exercise-q">{exercise.prompt}</p>

      {exercise.matrix && <MatrixRenderer matrix={exercise.matrix} />}

      <input
        className="answer-field"
        type="text"
        placeholder={exercise.answerPlaceholder ?? ''}
        value={answer}
        onChange={(event) => setAnswer(event.target.value)}
      />
      <Button onClick={handleSubmit}>Submit</Button>
    </div>
  );
}
