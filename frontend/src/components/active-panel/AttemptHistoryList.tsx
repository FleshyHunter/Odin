import type { Attempt } from '../../types';
import { AttemptHistoryRow } from './AttemptHistoryRow';

interface AttemptHistoryListProps {
  attempts: Attempt[];
}

// Generic and reusable: takes attempts as data only, no concept/node
// awareness baked in — drops into NodeDetail today, reusable anywhere
// else attempt history needs to be shown later without rebuilding it.
export function AttemptHistoryList({ attempts }: AttemptHistoryListProps) {
  if (attempts.length === 0) {
    return <p className="panel-footnote">No attempts yet on this concept.</p>;
  }

  return (
    <div className="attempt-list">
      {attempts.map((attempt) => (
        <AttemptHistoryRow key={attempt.id} attempt={attempt} />
      ))}
    </div>
  );
}
