// deferred.md #94: 'locked' added alongside the mockup's original three —
// journey_concepts.status genuinely distinguishes "reachable now"
// (available -> pending here) from "not reachable yet" (locked), and
// PRD.md's locked "Prerequisite Philosophy" (line 260, "warn if
// prerequisites unmet") calls for surfacing exactly that, not collapsing
// the two into one dim state.
export type ConceptStatus = 'mastered' | 'current' | 'pending' | 'locked';

export interface RoadmapNode {
  kind: 'node';
  id: string;
  title: string;
  status: ConceptStatus;
  isJunction?: boolean;
  // Secondary line under the title — junction nodes use it for the
  // second prerequisite ("+ Vector spaces"), the current node uses it
  // for "you are here". Never both at once in the mockup.
  subtitle?: string;
  // Titles of this node's NOT-YET-mastered prerequisites — empty for a
  // node with no unmet prerequisites. Only meaningful for a 'locked'
  // node (NodeDetail's advisory line); carried on every node rather than
  // computed lazily since buildRoadmapData.ts already has every other
  // node's mapped status in scope when building this one.
  unmetPrerequisiteTitles?: string[];
  // deferred.md #38 — KIV Review's real data, straight off the same
  // journey_concepts row the rest of this node already comes from.
  foundationGap: boolean;
  kivFlagged: boolean;
  masteryScore?: number;
}

export interface CollapsedGroupItem {
  kind: 'collapsed';
  id: string;
  // Pre-split display lines (SVG text has no auto-wrap) — e.g.
  // ["5 foundational", "concepts"].
  lines: string[];
  // The concepts this summary stands in for — revealed in place when
  // expanded (deferred.md #42). Omit for a collapsed item with nothing
  // to expand into (e.g. a synthetic "Collapse" control, see
  // RoadmapCanvas's expandItems()).
  concepts?: { id: string; title: string }[];
}

export type RoadmapItem = RoadmapNode | CollapsedGroupItem;

export interface RoadmapData {
  trackTitle: string;
  masteredCount: number;
  totalCount: number;
  items: RoadmapItem[];
  // deferred.md #38 — flat, pre-collapse list of KIV-flagged, not-yet-
  // mastered nodes. Deliberately separate from `items`: a KIV node could
  // never actually end up inside a collapsed group in practice (only
  // mastered runs collapse), but computing this from the raw per-node
  // list rather than the post-collapse one keeps that guarantee real
  // rather than assumed.
  kivItems: RoadmapNode[];
}
