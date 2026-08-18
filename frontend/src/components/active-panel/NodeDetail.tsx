import { useEffect, useState } from 'react';
import type { Attempt, Difficulty } from '../../types';
import * as exercisesApi from '../../api/exercises';
import { AttemptHistoryList } from './AttemptHistoryList';

interface NodeDetailProps {
  nodeId: string;
  nodeTitle: string;
  onBack: () => void;
  onAttempt: (difficulty: Difficulty) => void;
}

const DIFFICULTIES: { value: Difficulty; label: string }[] = [
  { value: 'basic', label: 'Basic' },
  { value: 'intermediate', label: 'Intermediate' },
  { value: 'advanced', label: 'Advanced' },
];

// Lives inside the Map tab's own space (Roadmap -> click a node -> this),
// not a separate page and not the "Now" tab — Now only ever shows
// whichever exercise is CURRENTLY being attempted, whether that got
// there from a live tutor offer or from picking a tier here. There's no
// bank of many distinct questions per node to browse (exercises are
// templates, max 3 per concept — one per difficulty — each
// re-instantiated fresh on request), so this is deliberately two small,
// real lists: tiers to attempt, and past attempts to review.
export function NodeDetail({ nodeId, nodeTitle, onBack, onAttempt }: NodeDetailProps) {
  const [attempts, setAttempts] = useState<Attempt[]>([]);

  useEffect(() => {
    let cancelled = false;
    exercisesApi.getNodeHistory(nodeId).then((result) => {
      if (!cancelled) setAttempts(result);
    });
    return () => {
      cancelled = true;
    };
  }, [nodeId]);

  return (
    <div className="node-detail">
      <button type="button" className="back-link" onClick={onBack}>
        ‹ Back to map
      </button>
      <h3 className="node-title">{nodeTitle}</h3>

      <p className="section-label">Practice</p>
      <div className="tier-row">
        {DIFFICULTIES.map((tier) => (
          <button key={tier.value} type="button" className="tier-btn" onClick={() => onAttempt(tier.value)}>
            {tier.label}
          </button>
        ))}
      </div>

      <p className="section-label">Past attempts</p>
      <AttemptHistoryList attempts={attempts} />
    </div>
  );
}
