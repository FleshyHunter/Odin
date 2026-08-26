import { useEffect, useState } from 'react';
import * as exercisesApi from '../../api/exercises';
import type { Attempt, Difficulty } from '../../types';
import type { ConceptStatus } from '../roadmap/types';
import { AttemptHistoryList } from './AttemptHistoryList';
import './nodeDetail.css';

interface NodeDetailProps {
  journeyId: string | null;
  nodeId: string;
  nodeTitle: string;
  status: ConceptStatus;
  unmetPrerequisiteTitles: string[];
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
// deferred.md #94: nodeId is now always a real concept_id (Roadmap.tsx's
// real DAG fetch), so the real history endpoint is meaningful again.
export function NodeDetail({ journeyId, nodeId, nodeTitle, status, unmetPrerequisiteTitles, onBack, onAttempt }: NodeDetailProps) {
  const [attempts, setAttempts] = useState<Attempt[]>([]);

  useEffect(() => {
    setAttempts([]);
    if (!journeyId) return;
    let cancelled = false;
    exercisesApi.getNodeHistory(journeyId, nodeId).then((result) => {
      if (!cancelled) setAttempts(result);
    });
    return () => {
      cancelled = true;
    };
  }, [journeyId, nodeId]);

  return (
    <div className="node-detail">
      <button type="button" className="back-link" onClick={onBack}>
        ‹ Back to map
      </button>
      <h3 className="node-title">{nodeTitle}</h3>

      {/* No hard gating — PRD.md's locked "Prerequisite Philosophy" (line
          260): "No prerequisite ever blocks navigation." Tiers below stay
          fully clickable regardless of status; this is advisory only
          ("warn if prerequisites unmet"), never a gate. */}
      {status === 'locked' && (
        <p className="node-advisory">
          Prerequisite not yet complete
          {unmetPrerequisiteTitles.length > 0 ? `: ${unmetPrerequisiteTitles.join(', ')}` : ''}.
        </p>
      )}

      <p className="node-section-label">Practice</p>
      <div className="tier-row">
        {DIFFICULTIES.map((tier) => (
          <button key={tier.value} type="button" className="tier-btn" onClick={() => onAttempt(tier.value)}>
            {tier.label}
          </button>
        ))}
      </div>

      <p className="node-section-label">Past attempts</p>
      <AttemptHistoryList attempts={attempts} />
    </div>
  );
}
