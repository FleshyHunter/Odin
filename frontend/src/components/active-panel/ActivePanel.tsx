import { useEffect, useState } from 'react';
import { getRoadmap } from '../../api/roadmap';
import type { Difficulty, Exercise, MasteryStatus, SubmitAnswerResult } from '../../types';
import { KivReview } from '../kiv/KivReview';
import { Roadmap } from '../roadmap/Roadmap';
import { buildRoadmapData } from '../roadmap/buildRoadmapData';
import type { ConceptStatus, RoadmapData } from '../roadmap/types';
import { ExerciseCard } from './ExerciseCard';
import { MasteryBar } from './MasteryBar';
import { NodeDetail } from './NodeDetail';
import './activePanel.css';

type PanelTab = 'now' | 'map' | 'kiv';

interface SelectedNode {
  id: string;
  title: string;
  status: ConceptStatus;
  unmetPrerequisiteTitles: string[];
}

interface ActivePanelProps {
  exercise: Exercise | null;
  mastery: MasteryStatus | null;
  onSubmitAnswer?: (answer: string) => Promise<SubmitAnswerResult | undefined>;
  // Map's node-detail tiers hand off to Now (see selectedNode below) —
  // this is what actually fetches/serves the fresh instantiated exercise;
  // ChatView owns the real exercise state, same as onSubmitAnswer already does.
  onStartAttempt?: (nodeId: string, nodeTitle: string, difficulty: Difficulty) => void;
  // deferred.md #38: ExerciseCard's "Move on for now" (shown after a
  // wrong advanced attempt) — bubbles up rather than calling the API
  // directly, since ChatView is what actually tracks which concept the
  // CURRENT exercise belongs to (exerciseConceptId, same reason its own
  // mastery-isolation fix needed that field). NodeDetail's own "Skip"
  // button doesn't need this — it already has journeyId/nodeId directly.
  onMoveOnFromExercise?: () => void;
  width: number;
  // deferred.md #94: real DAG fetch, replacing sampleData.ts.
  journeyId: string | null;
  trackTitle: string;
}

export function ActivePanel({
  exercise,
  mastery,
  onSubmitAnswer,
  onStartAttempt,
  onMoveOnFromExercise,
  width,
  journeyId,
  trackTitle,
}: ActivePanelProps) {
  const [tab, setTab] = useState<PanelTab>('now');
  // Map's own drill-down state: null = DAG view, set = a specific node's
  // detail (tiers + history). Lives here, not in Roadmap itself, since
  // attempting a tier needs to reach across into the Now tab.
  const [selectedNode, setSelectedNode] = useState<SelectedNode | null>(null);
  const [roadmap, setRoadmap] = useState<RoadmapData | null>(null);

  // deferred.md #94: fetch-on-mount/switch only, no live push — same
  // posture as ChatView's own mastery-bar effect. A node flipping status
  // mid-session while the Map tab sits in the background (e.g. an
  // attempt made from the Now tab completing a concept) won't be
  // reflected until journeyId changes again; explicitly out of scope for
  // this pass.
  useEffect(() => {
    setRoadmap(null);
    setSelectedNode(null);
    if (!journeyId) return;
    let cancelled = false;
    getRoadmap(journeyId).then((nodes) => {
      if (!cancelled) setRoadmap(buildRoadmapData(nodes, trackTitle));
    });
    return () => {
      cancelled = true;
    };
  }, [journeyId, trackTitle]);

  const handleAttempt = (difficulty: Difficulty) => {
    if (!selectedNode) return;
    onStartAttempt?.(selectedNode.id, selectedNode.title, difficulty);
    // Map stays purely for browsing — the moment you commit to actually
    // attempting something, Now becomes the single surface for it,
    // whether it got there from a live tutor offer or from here.
    setTab('now');
  };

  // Shared by both the Map's onNodeClick and KIV's "Review" — same
  // drill-down destination (NodeDetail's tier/attempt flow) either way,
  // deferred.md #38. Switches to the Map tab so NodeDetail (gated on
  // tab === 'map') actually renders.
  const handleReviewConcept = (id: string, title: string, status: ConceptStatus, unmetPrerequisiteTitles: string[]) => {
    setSelectedNode({ id, title, status, unmetPrerequisiteTitles });
    setTab('map');
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
          {exercise && <ExerciseCard exercise={exercise} onSubmit={onSubmitAnswer} onMoveOn={onMoveOnFromExercise} />}
          {mastery && <MasteryBar conceptTitle={mastery.conceptTitle} masteryScore={mastery.masteryScore} />}
          {!exercise && !mastery && (
            <p className="panel-footnote">No exercise yet for this track.</p>
          )}
        </>
      )}

      {tab === 'map' &&
        (selectedNode ? (
          <NodeDetail
            journeyId={journeyId}
            nodeId={selectedNode.id}
            nodeTitle={selectedNode.title}
            status={selectedNode.status}
            unmetPrerequisiteTitles={selectedNode.unmetPrerequisiteTitles}
            onBack={() => setSelectedNode(null)}
            onAttempt={handleAttempt}
          />
        ) : roadmap ? (
          <Roadmap data={roadmap} onNodeClick={handleReviewConcept} />
        ) : (
          // Deliberately NOT sampleRoadmap as a loading fallback — showing
          // sample data over a real loading state would let a user click a
          // node that doesn't exist (deferred.md #94).
          <p className="panel-footnote">{journeyId ? 'Loading map…' : 'No journey yet for this track.'}</p>
        ))}

      {tab === 'kiv' && <KivReview items={roadmap?.kivItems ?? []} onReview={handleReviewConcept} />}
    </aside>
  );
}
