import { COLLAPSED_HEIGHT } from '../nodes/CollapsedNode';
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

const CENTER_X = 170;
const VIEWBOX_WIDTH = 340;
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
// connecting edge segments between consecutive items. Fixed x=170
// center column and ~58px spacing, matching odin_map_view.html's
// 340-wide viewBox — extra room is reserved around the taller
// collapsed-chain rect.
export function layoutRoadmap(items: RoadmapItem[]): RoadmapLayout {
  const positions: PositionedItem[] = [];
  let y = FIRST_Y;

  for (const item of items) {
    positions.push({ item, x: CENTER_X, y });
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
      x: CENTER_X,
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
