import type { KeyboardEvent } from 'react';
import './nodes.css';

interface CollapsedNodeProps {
  x: number;
  y: number;
  lines: string[];
  onToggle: () => void;
  expanded: boolean;
}

// Deliberately a different shape (dashed rect, not a circle) so it
// reads as "several concepts, summarized" rather than one more node in
// the chain — per the mockup's own comment.
export const COLLAPSED_WIDTH = 84;
export const COLLAPSED_HEIGHT = 30;
const LINE_HEIGHT = 11;

// deferred.md #42: this was styled `cursor: pointer` and captioned "tap
// to expand" in the Legend, but had no onClick anywhere — clicking did
// nothing. `onToggle`/`expanded` wired in here; RoadmapCanvas.expandItems()
// decides what "expanded" actually renders (the group's real concepts,
// plus a same-id "Collapse" control reusing this same component).
export function CollapsedNode({ x, y, lines, onToggle, expanded }: CollapsedNodeProps) {
  const firstLineY = y - ((lines.length - 1) * LINE_HEIGHT) / 2 + 4;

  const handleKeyDown = (event: KeyboardEvent<SVGGElement>) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onToggle();
    }
  };

  return (
    <g
      className="roadmap-collapsed-node"
      onClick={onToggle}
      onKeyDown={handleKeyDown}
      role="button"
      tabIndex={0}
      aria-expanded={expanded}
    >
      <rect
        x={x - COLLAPSED_WIDTH / 2}
        y={y - COLLAPSED_HEIGHT / 2}
        width={COLLAPSED_WIDTH}
        height={COLLAPSED_HEIGHT}
        rx={8}
        className="roadmap-collapsed-rect"
      />
      {lines.map((line, i) => (
        <text key={line} x={x} y={firstLineY + i * LINE_HEIGHT} textAnchor="middle" className="roadmap-collapsed-text">
          {line}
        </text>
      ))}
    </g>
  );
}
