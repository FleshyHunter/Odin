export type ConceptStatus = 'mastered' | 'current' | 'pending';

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
}
