import { useState } from 'react';
import type { Exercise, MasteryStatus } from '../../types';
import { ExerciseCard } from './ExerciseCard';
import { MasteryBar } from './MasteryBar';

type PanelTab = 'now' | 'map';

interface ActivePanelProps {
  exercise: Exercise | null;
  mastery: MasteryStatus | null;
  onSubmitAnswer?: (answer: string) => void;
}

export function ActivePanel({ exercise, mastery, onSubmitAnswer }: ActivePanelProps) {
  const [tab, setTab] = useState<PanelTab>('now');

  return (
    <aside className="active-panel">
      <div className="panel-tabs">
        <button className={tab === 'now' ? 'tab active' : 'tab'} onClick={() => setTab('now')}>
          Now
        </button>
        <button className={tab === 'map' ? 'tab active' : 'tab'} onClick={() => setTab('map')}>
          Map
        </button>
      </div>

      {tab === 'now' && (
        <>
          {exercise && <ExerciseCard exercise={exercise} onSubmit={onSubmitAnswer} />}
          {mastery && <MasteryBar conceptTitle={mastery.conceptTitle} masteryScore={mastery.masteryScore} />}
          {!exercise && !mastery && (
            <p className="panel-footnote">No exercise yet for this track.</p>
          )}
        </>
      )}

      {tab === 'map' && (
        // NC4 (ARCHITECTURE_LOCK.md, Remaining Unresolved): the Map tab's
        // real content (journey DAG view) and its placement relative to
        // the Tracks/Projects sidebar structure was never re-confirmed
        // after the sidebar redesign. Placeholder only — per the handoff's
        // explicit instruction not to invent the real DAG view here.
        <div className="panel-map-placeholder">
          <p className="panel-footnote">
            Journey map view — not yet designed (see ARCHITECTURE_LOCK.md, NC4).
          </p>
        </div>
      )}
    </aside>
  );
}
