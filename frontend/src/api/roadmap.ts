import { apiFetch } from './client';

export interface RoadmapApiNode {
  conceptId: string;
  title: string;
  // Raw DB status ("locked"/"available"/"in_progress"/"complete") —
  // buildRoadmapData.ts maps it to the frontend's own ConceptStatus.
  status: string;
  prerequisiteIds: string[];
}

interface RoadmapNodeBody {
  concept_id: string;
  title: string;
  status: string;
  prerequisite_ids: string[];
}

function toRoadmapNode(body: RoadmapNodeBody): RoadmapApiNode {
  return {
    conceptId: body.concept_id,
    title: body.title,
    status: body.status,
    prerequisiteIds: body.prerequisite_ids,
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
