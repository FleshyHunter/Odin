import { useState } from 'react';
import type { Exercise, MasteryStatus } from '../../types';
import { Roadmap } from '../roadmap/Roadmap';
import { ExerciseCard } from './ExerciseCard';
import { MasteryBar } from './MasteryBar';

type PanelTab = 'now' | 'map';

interface ActivePanelProps {
  exercise: Exercise | null;
  mastery: MasteryStatus | null;
  onSubmitAnswer?: (answer: string) => void;
  width: number;
}

export function ActivePanel({ exercise, mastery, onSubmitAnswer, width }: ActivePanelProps) {
  const [tab, setTab] = useState<PanelTab>('now');

  return (
    <aside className="active-panel" style={{ width }}>
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

      {tab === 'map' && <Roadmap />}
    </aside>
  );
}
