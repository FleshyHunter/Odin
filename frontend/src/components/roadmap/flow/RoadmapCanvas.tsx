import { useEffect, useRef, useState } from 'react';
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
// zoomFraction (0..1) is a viewport-relative position, not an absolute
// scale — decouples the +/- buttons (fixed steps in this space) from
// the real scale factor, which depends on the container's actual
// measured width (see below) and can shift on its own when the panel
// is resized. 0 = min zoom (more of the chain visible, at a scale
// below "fills the width exactly"); 1 = max zoom (content width
// exactly fills the real viewport, zero blank margin — the natural
// ceiling, not a guessed constant).
const ZOOM_STEP_FRACTION = 0.25;
// S_MIN = S_MAX * MIN_ZOOM_RATIO — how much smaller the zoomed-out
// scale is than the "fills exactly" ceiling. Below 1, always.
const MIN_ZOOM_RATIO = 0.7;

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
      // Not real journey_concepts data (#42's pre-existing simplification,
      // untouched here) — never KIV-eligible.
      foundationGap: false,
      kivFlagged: false,
    }));
    const collapseControl: RoadmapItem = { kind: 'collapsed', id: item.id, lines: ['Collapse'] };
    return [...revealedNodes, collapseControl];
  });
}

export function RoadmapCanvas({ items, onNodeClick }: RoadmapCanvasProps) {
  const [expandedGroupIds, setExpandedGroupIds] = useState<Set<string>>(new Set());
  // 0 = zoomed out (see more of the chain at once); 1 = zoomed in (content
  // width exactly fills the real viewport). Defaults to max zoom (1).
  const [zoomFraction, setZoomFraction] = useState(1);
  const [viewportWidthPx, setViewportWidthPx] = useState<number | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Tracks the SCROLL element's real width (roadmap-canvas-wrap's own
  // inner width, effectively) — a ResizeObserver, not a window resize
  // listener, since ActivePanel's width changes from a manual drag, not
  // a window-level event.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const observer = new ResizeObserver((entries) => {
      const width = entries[0]?.contentRect.width;
      if (width) setViewportWidthPx(width);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

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

  const { positions, edges, viewBoxWidth: contentWidthUnits, viewBoxHeight } = layoutRoadmap(
    expandItems(items, expandedGroupIds),
  );

  // Before the first ResizeObserver callback lands, fall back to
  // rendering the content at its own natural width/scale (S == 1) —
  // avoids a zero-size flash, and is a perfectly reasonable first paint.
  const scale = viewportWidthPx ? viewportWidthPx / contentWidthUnits : null;
  const sMax = scale;
  const sMin = sMax !== null ? sMax * MIN_ZOOM_RATIO : null;
  const s = sMax !== null && sMin !== null ? sMin + (sMax - sMin) * zoomFraction : 1;
  const svgWidthPx = viewportWidthPx ?? contentWidthUnits;
  const renderedViewBoxWidth = viewportWidthPx ? viewportWidthPx / s : contentWidthUnits;
  const renderedHeightPx = viewBoxHeight * s;

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
          aria-label="Zoom in"
          disabled={zoomFraction >= 1}
          onClick={() => setZoomFraction((f) => Math.min(1, +(f + ZOOM_STEP_FRACTION).toFixed(2)))}
        >
          +
        </button>
        <button
          type="button"
          className="roadmap-zoom-btn"
          aria-label="Zoom out"
          disabled={zoomFraction <= 0}
          onClick={() => setZoomFraction((f) => Math.max(0, +(f - ZOOM_STEP_FRACTION).toFixed(2)))}
        >
          −
        </button>
      </div>

      {/* width is always the real measured viewport width (never wider) —
          horizontal scroll is structurally impossible, not just hidden.
          viewBox's width portion is what scales with zoom instead
          (renderedViewBoxWidth, always >= contentWidthUnits by
          construction) — the standard SVG "camera zoom" pattern. height
          uses the SAME scale `s` on both the pixel value and the
          viewBox's own height-in-units ratio, so x/y stay uniformly
          scaled — circles stay circular, text isn't stretched. */}
      <div className="roadmap-canvas-scroll" ref={scrollRef}>
        <svg
          width={svgWidthPx}
          height={renderedHeightPx}
          viewBox={`0 0 ${renderedViewBoxWidth} ${viewBoxHeight}`}
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
