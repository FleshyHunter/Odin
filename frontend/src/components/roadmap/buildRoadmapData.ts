import type { RoadmapApiNode } from '../../api/roadmap';
import type { ConceptStatus, RoadmapData, RoadmapItem, RoadmapNode } from './types';

const COLLAPSE_MIN_RUN = 3;

function mapStatus(status: string): ConceptStatus {
  switch (status) {
    case 'complete':
      return 'mastered';
    // 'in_progress' — the ONE concept a real thread is actively teaching
    // right now (journeys/turn.rs's UPDATE ... SET status = 'in_progress'
    // sites) — is the real, durable signal for "current", not a
    // client-side currentConceptId comparison: study_threads.current_
    // concept_id is only ever set once at thread-creation time and never
    // updated afterward, so it can't be trusted to track advancement
    // within a session the way journey_concepts.status actually does.
    case 'in_progress':
      return 'current';
    case 'locked':
      return 'locked';
    default:
      // 'available' (the common case), plus any future/unexpected value —
      // fail toward the least alarming rendering rather than a crash.
      return 'pending';
  }
}

// Groups a maximal run of 3+ consecutive mastered, non-junction nodes
// into one CollapsedGroupItem (RoadmapCanvas/#42 already builds the
// expand/collapse interaction on top of this shape) — everything else
// passes through unchanged.
function collapseMasteredRuns(nodes: RoadmapNode[]): RoadmapItem[] {
  const result: RoadmapItem[] = [];
  let run: RoadmapNode[] = [];

  const flushRun = () => {
    if (run.length >= COLLAPSE_MIN_RUN) {
      result.push({
        kind: 'collapsed',
        id: `collapsed-${run[0].id}`,
        lines: [`${run.length} foundational`, 'concepts'],
        concepts: run.map((node) => ({ id: node.id, title: node.title })),
      });
    } else {
      result.push(...run);
    }
    run = [];
  };

  for (const node of nodes) {
    if (node.status === 'mastered' && !node.isJunction) {
      run.push(node);
    } else {
      flushRun();
      result.push(node);
    }
  }
  flushRun();
  return result;
}

// Pure, unit-testable: turns the real topologically-ordered API response
// into Roadmap's own presentation shape (status mapping, junction/
// subtitle detection, mastered-run collapsing) — the one place
// sampleData.ts's hand-authored shape gets produced for real data
// instead (deferred.md #94).
export function buildRoadmapData(apiNodes: RoadmapApiNode[], trackTitle: string): RoadmapData {
  const titleById = new Map(apiNodes.map((n) => [n.conceptId, n.title]));
  const statusById = new Map(apiNodes.map((n) => [n.conceptId, mapStatus(n.status)]));

  const nodes: RoadmapNode[] = apiNodes.map((apiNode, index) => {
    const status = mapStatus(apiNode.status);
    const precedingId = index > 0 ? apiNodes[index - 1].conceptId : null;
    const otherPrereqIds = apiNode.prerequisiteIds.filter((id) => id !== precedingId);
    const isJunction = apiNode.prerequisiteIds.length >= 2 && otherPrereqIds.length > 0;
    const unmetPrerequisiteTitles = apiNode.prerequisiteIds
      .filter((id) => statusById.get(id) !== 'mastered')
      .map((id) => titleById.get(id) ?? id);

    return {
      kind: 'node',
      id: apiNode.conceptId,
      title: apiNode.title,
      status,
      isJunction,
      subtitle:
        status === 'current'
          ? 'you are here'
          : isJunction
            ? `+ ${titleById.get(otherPrereqIds[0]) ?? ''}`
            : undefined,
      unmetPrerequisiteTitles,
      foundationGap: apiNode.foundationGap,
      kivFlagged: apiNode.kivFlagged,
      masteryScore: apiNode.masteryScore,
    };
  });

  return {
    trackTitle,
    masteredCount: nodes.filter((node) => node.status === 'mastered').length,
    totalCount: nodes.length,
    items: collapseMasteredRuns(nodes),
    // deferred.md #38 — from the pre-collapse list, not `items` above
    // (see RoadmapData.kivItems's own doc comment on why that matters).
    kivItems: nodes.filter((node) => node.kivFlagged && node.status !== 'mastered'),
  };
}
