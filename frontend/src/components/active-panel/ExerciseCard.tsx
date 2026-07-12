import { useState } from 'react';
import type { Exercise } from '../../types';
import { Button } from '../ui/Button/Button';
import { MatrixRenderer } from './MatrixRenderer';

interface ExerciseCardProps {
  exercise: Exercise;
  onSubmit?: (answer: string) => void;
}

function capitalize(word: string): string {
  return word.charAt(0).toUpperCase() + word.slice(1);
}

export function ExerciseCard({ exercise, onSubmit }: ExerciseCardProps) {
  const [answer, setAnswer] = useState('');

  const handleSubmit = () => {
    if (!answer.trim()) return;
    onSubmit?.(answer.trim());
  };

  return (
    <div className="exercise-card">
      <span className="difficulty-tag">{capitalize(exercise.difficulty)}</span>
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
