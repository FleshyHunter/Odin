import { COLLAPSED_HEIGHT, COLLAPSED_WIDTH } from '../nodes/CollapsedNode';
import { OUTER_R, OUTER_R_JUNCTION } from '../nodes/TargetNode';
import type { RoadmapItem } from '../types';

export interface PositionedItem {
  item: RoadmapItem;
  x: number;
  y: number;
}

export interface EdgeSegment {
  id: string;
  x: number;
  y1: number;
  y2: number;
  traversed: boolean;
}

export interface RoadmapLayout {
  positions: PositionedItem[];
  edges: EdgeSegment[];
  viewBoxWidth: number;
  viewBoxHeight: number;
}

// Left margin for the shared node/edge column — sized to clear the
// WIDEST item's own half-extent so nothing clips against the left
// edge. CollapsedNode draws a box centered on this same x (84 wide,
// half=42), which is wider than either dot radius (OUTER_R_JUNCTION is
// only 14) — that's the binding constraint, not the dots.
const NODE_X = Math.max(OUTER_R_JUNCTION, COLLAPSED_WIDTH / 2) + 8;
// Room for the longest realistic label to the right of NODE_X — SVG
// text has no auto-wrap (see CollapsedGroupItem's own comment), so
// this is a fixed generous budget, not a measured one. Moving the
// column from a centered 170 to a left-anchored NODE_X actually frees
// UP more of this width for labels than before, not less.
const VIEWBOX_WIDTH = 230;
const FIRST_Y = 32;
const SPACING = 58;
const COLLAPSED_EXTRA_GAP = 10;
const EDGE_GAP = 8;
const BOTTOM_MARGIN = 40;

function itemHalfExtent(item: RoadmapItem): number {
  if (item.kind === 'collapsed') return COLLAPSED_HEIGHT / 2;
  return item.isJunction ? OUTER_R_JUNCTION : OUTER_R;
}

// Pure layout math, kept separate from rendering: given the ordered
// concept chain, compute each item's vertical position and the
// connecting edge segments between consecutive items. Every item
// shares the same x (NODE_X, flush toward the left edge rather than
// centered) — TargetNode's labels already always extend rightward from
// their dot, so a left-anchored column is what gives them room to
// breathe instead of dead space on one side and cramped text on the
// other.
export function layoutRoadmap(items: RoadmapItem[]): RoadmapLayout {
  const positions: PositionedItem[] = [];
  let y = FIRST_Y;

  for (const item of items) {
    positions.push({ item, x: NODE_X, y });
    y += SPACING + (item.kind === 'collapsed' ? COLLAPSED_EXTRA_GAP : 0);
  }

  const edges: EdgeSegment[] = [];
  for (let i = 0; i < positions.length - 1; i += 1) {
    const from = positions[i];
    const to = positions[i + 1];
    // Gold if the segment leads away from a mastered/current node —
    // matches the mockup, where the edge just past "current" is still
    // gold even though it leads toward an unreached node.
    const traversed = from.item.kind === 'node' && (from.item.status === 'mastered' || from.item.status === 'current');

    edges.push({
      id: `edge-${from.item.id}-${to.item.id}`,
      x: NODE_X,
      y1: from.y + itemHalfExtent(from.item) + EDGE_GAP,
      y2: to.y - itemHalfExtent(to.item) - EDGE_GAP,
      traversed,
    });
  }

  const lastY = positions[positions.length - 1]?.y ?? FIRST_Y;

  return {
    positions,
    edges,
    viewBoxWidth: VIEWBOX_WIDTH,
    viewBoxHeight: lastY + BOTTOM_MARGIN,
  };
}
