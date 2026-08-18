import { useState } from 'react';
import type { Difficulty, Exercise, MasteryStatus, SubmitAnswerResult } from '../../types';
import { KivReview } from '../kiv/KivReview';
import { Roadmap } from '../roadmap/Roadmap';
import { ExerciseCard } from './ExerciseCard';
import { MasteryBar } from './MasteryBar';
import { NodeDetail } from './NodeDetail';

type PanelTab = 'now' | 'map' | 'kiv';

interface ActivePanelProps {
  exercise: Exercise | null;
  mastery: MasteryStatus | null;
  onSubmitAnswer?: (answer: string) => Promise<SubmitAnswerResult | undefined>;
  // Map's node-detail tiers hand off to Now (see selectedNode below) —
  // this is what actually fetches/serves the fresh instantiated exercise;
  // ChatView owns the real exercise state, same as onSubmitAnswer already does.
  onStartAttempt?: (nodeId: string, difficulty: Difficulty) => void;
  width: number;
}

export function ActivePanel({ exercise, mastery, onSubmitAnswer, onStartAttempt, width }: ActivePanelProps) {
  const [tab, setTab] = useState<PanelTab>('now');
  // Map's own drill-down state: null = DAG view, set = a specific node's
  // detail (tiers + history). Lives here, not in Roadmap itself, since
  // attempting a tier needs to reach across into the Now tab.
  const [selectedNode, setSelectedNode] = useState<{ id: string; title: string } | null>(null);

  const handleAttempt = (difficulty: Difficulty) => {
    if (!selectedNode) return;
    onStartAttempt?.(selectedNode.id, difficulty);
    // Map stays purely for browsing — the moment you commit to actually
    // attempting something, Now becomes the single surface for it,
    // whether it got there from a live tutor offer or from here.
    setTab('now');
  };

  return (
    <aside className="active-panel" style={{ width }}>
      <div className="panel-tabs">
        <button className={tab === 'now' ? 'tab active' : 'tab'} onClick={() => setTab('now')}>
          Now
        </button>
        <button className={tab === 'map' ? 'tab active' : 'tab'} onClick={() => setTab('map')}>
          Map
        </button>
        <button className={tab === 'kiv' ? 'tab active' : 'tab'} onClick={() => setTab('kiv')}>
          KIV
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

      {tab === 'map' &&
        (selectedNode ? (
          <NodeDetail
            nodeId={selectedNode.id}
            nodeTitle={selectedNode.title}
            onBack={() => setSelectedNode(null)}
            onAttempt={handleAttempt}
          />
        ) : (
          <Roadmap onNodeClick={(id, title) => setSelectedNode({ id, title })} />
        ))}

      {tab === 'kiv' && <KivReview />}
    </aside>
  );
}
