import { useState } from 'react';
import type { Attempt } from '../../types';

interface AttemptHistoryRowProps {
  attempt: Attempt;
}

function capitalize(word: string): string {
  return word.charAt(0).toUpperCase() + word.slice(1);
}

// Reuses KIV's row visual pattern (row + colored tag + meta line —
// kiv.css's .kiv-row/.kiv-tag/.kiv-meta) since the shape reads the same,
// but this is a genuinely different component/data: KivItem is
// concept-level (title, severity, a continuous mastery score needing a
// progress bar); an Attempt is one past exercise attempt (a question,
// correct/incorrect — binary, no bar makes sense here). Styled
// consistently, not literally reused.
export function AttemptHistoryRow({ attempt }: AttemptHistoryRowProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="attempt-row">
      <div className="attempt-row-main">
        <div className="attempt-row-top">
          <span className="attempt-title">{attempt.title}</span>
          <span className={attempt.isCorrect ? 'attempt-tag attempt-tag-correct' : 'attempt-tag attempt-tag-incorrect'}>
            {attempt.isCorrect ? 'Correct' : 'Incorrect'}
          </span>
        </div>
        <span className="attempt-meta">
          {attempt.date} · {capitalize(attempt.difficulty)}
        </span>
        <div className={expanded ? 'attempt-desc expanded' : 'attempt-desc'}>{attempt.question}</div>
        <button type="button" className="attempt-toggle" onClick={() => setExpanded((prev) => !prev)}>
          {expanded ? 'Show less' : 'Show more'}
        </button>
      </div>
    </div>
  );
}
