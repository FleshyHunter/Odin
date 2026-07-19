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
}

export type RoadmapItem = RoadmapNode | CollapsedGroupItem;

export interface RoadmapData {
  trackTitle: string;
  masteredCount: number;
  totalCount: number;
  items: RoadmapItem[];
}
