import { apiFetch } from './client';

export interface RoadmapApiNode {
  conceptId: string;
  title: string;
  // Raw DB status ("locked"/"available"/"in_progress"/"complete") —
  // buildRoadmapData.ts maps it to the frontend's own ConceptStatus.
  status: string;
  prerequisiteIds: string[];
  // deferred.md #38 — carried here rather than a separate KIV-list
  // endpoint: the Map already fetches every real journey_concepts row
  // for this journey, so the KIV tab reuses this same response instead
  // of a second, duplicate query.
  foundationGap: boolean;
  kivFlagged: boolean;
  // undefined if mastery_bank has no row yet (never attempted) — real
  // 0 is a different, meaningful state from "no data at all."
  masteryScore?: number;
}

interface RoadmapNodeBody {
  concept_id: string;
  title: string;
  status: string;
  prerequisite_ids: string[];
  foundation_gap: boolean;
  kiv_flagged: boolean;
  mastery_score: number | null;
}

function toRoadmapNode(body: RoadmapNodeBody): RoadmapApiNode {
  return {
    conceptId: body.concept_id,
    title: body.title,
    status: body.status,
    prerequisiteIds: body.prerequisite_ids,
    foundationGap: body.foundation_gap,
    kivFlagged: body.kiv_flagged,
    masteryScore: body.mastery_score ?? undefined,
  };
}

// GET /journeys/{journeyId}/roadmap (backend/src/journeys/handlers.rs) —
// deferred.md #94. Response is already in topological order (entry
// concept first) — buildRoadmapData.ts relies on that ordering for
// junction/collapsing logic, not just for display.
export async function getRoadmap(journeyId: string): Promise<RoadmapApiNode[]> {
  const body = await apiFetch<RoadmapNodeBody[]>(`/journeys/${journeyId}/roadmap`);
  return body.map(toRoadmapNode);
}
