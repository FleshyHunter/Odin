import { useState } from 'react';
import { Edge } from '../edges/Edge';
import { PulseRing } from '../effects/PulseRing';
import { CollapsedNode } from '../nodes/CollapsedNode';
import { OUTER_R, OUTER_R_JUNCTION, TargetNode } from '../nodes/TargetNode';
import type { ConceptStatus, RoadmapItem } from '../types';
import { layoutRoadmap } from './layout';

interface RoadmapCanvasProps {
  items: RoadmapItem[];
  onNodeClick?: (id: string, title: string, status: ConceptStatus, unmetPrerequisiteTitles: string[]) => void;
}

const RING_GAP = 6;
// zoom is a plain multiplier on the SVG's own authored/native unit
// size (node radii, label font-size, item spacing all render at
// exactly their authored values at 1.0) — a pure camera control, not a
// layout recompute: layoutRoadmap() never re-runs on zoom change, only
// how large the same fixed layout renders. Bounds are illustrative,
// not exact — easy to retune once it's visually in front of you.
const ZOOM_MIN = 0.6;
const ZOOM_MAX = 1.6;
const ZOOM_STEP = 0.2;

// deferred.md #42: a collapsed group in `expandedGroupIds` is swapped
// for its real concepts (as plain pending nodes) plus a same-id
// "Collapse" control — reusing CollapsedNode/onToggle for both
// directions, since toggling by id is symmetric either way. A group
// with no `concepts` (nothing to expand into) or not currently
// expanded passes through unchanged.
function expandItems(items: RoadmapItem[], expandedGroupIds: Set<string>): RoadmapItem[] {
  return items.flatMap((item): RoadmapItem[] => {
    if (item.kind !== 'collapsed' || !item.concepts || !expandedGroupIds.has(item.id)) {
      return [item];
    }
    const revealedNodes: RoadmapItem[] = item.concepts.map((concept) => ({
      kind: 'node',
      id: concept.id,
      title: concept.title,
      status: 'pending',
    }));
    const collapseControl: RoadmapItem = { kind: 'collapsed', id: item.id, lines: ['Collapse'] };
    return [...revealedNodes, collapseControl];
  });
}

export function RoadmapCanvas({ items, onNodeClick }: RoadmapCanvasProps) {
  const [expandedGroupIds, setExpandedGroupIds] = useState<Set<string>>(new Set());
  const [zoom, setZoom] = useState(1);

  const toggleGroup = (id: string) => {
    setExpandedGroupIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const { positions, edges, viewBoxWidth, viewBoxHeight } = layoutRoadmap(expandItems(items, expandedGroupIds));

  return (
    <>
      {/* Pinned to roadmap-canvas-wrap (its positioned ancestor), NOT
          inside roadmap-canvas-scroll below — so it stays put at a fixed
          corner regardless of scroll position, rather than scrolling away
          with the content underneath it. */}
      <div className="roadmap-zoom-controls">
        <button
          type="button"
          className="roadmap-zoom-btn"
          aria-label="Zoom out"
          disabled={zoom <= ZOOM_MIN}
          onClick={() => setZoom((z) => Math.max(ZOOM_MIN, +(z - ZOOM_STEP).toFixed(2)))}
        >
          −
        </button>
        <button
          type="button"
          className="roadmap-zoom-btn"
          aria-label="Zoom in"
          disabled={zoom >= ZOOM_MAX}
          onClick={() => setZoom((z) => Math.min(ZOOM_MAX, +(z + ZOOM_STEP).toFixed(2)))}
        >
          +
        </button>
      </div>

      {/* Real pixel width/height (not 100%/100%) so viewBox maps 1:1 to
          the chain's true coordinates — zoom directly scales the SVG's
          own rendered size, and roadmap-canvas-scroll's overflow:auto
          picks up native scrollbars whenever that exceeds the wrap's
          box, at any zoom level (not just as an overflow edge case). */}
      <div className="roadmap-canvas-scroll">
        <svg
          width={viewBoxWidth * zoom}
          height={viewBoxHeight * zoom}
          viewBox={`0 0 ${viewBoxWidth} ${viewBoxHeight}`}
          role="img"
          aria-label="Journey map"
        >
          <g>
            {edges.map((edge) => (
              <Edge key={edge.id} x={edge.x} y1={edge.y1} y2={edge.y2} traversed={edge.traversed} />
            ))}
          </g>

          {positions.map(({ item, x, y }) => {
            if (item.kind === 'collapsed') {
              return (
                <CollapsedNode
                  key={item.id}
                  x={x}
                  y={y}
                  lines={item.lines}
                  expanded={expandedGroupIds.has(item.id)}
                  onToggle={() => toggleGroup(item.id)}
                />
              );
            }

            const outerR = item.isJunction ? OUTER_R_JUNCTION : OUTER_R;

            return (
              <g key={item.id}>
                {item.status === 'current' && <PulseRing x={x} y={y} radius={outerR + RING_GAP} />}
                <TargetNode
                  x={x}
                  y={y}
                  title={item.title}
                  subtitle={item.subtitle}
                  status={item.status}
                  isJunction={item.isJunction}
                  onClick={
                    onNodeClick
                      ? () => onNodeClick(item.id, item.title, item.status, item.unmetPrerequisiteTitles ?? [])
                      : undefined
                  }
                />
              </g>
            );
          })}
        </svg>
      </div>
    </>
  );
}
