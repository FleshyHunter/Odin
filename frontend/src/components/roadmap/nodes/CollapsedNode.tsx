import './nodes.css';

interface CollapsedNodeProps {
  x: number;
  y: number;
  lines: string[];
}

// Deliberately a different shape (dashed rect, not a circle) so it
// reads as "several concepts, summarized" rather than one more node in
// the chain — per the mockup's own comment.
export const COLLAPSED_WIDTH = 84;
export const COLLAPSED_HEIGHT = 30;
const LINE_HEIGHT = 11;

export function CollapsedNode({ x, y, lines }: CollapsedNodeProps) {
  const firstLineY = y - ((lines.length - 1) * LINE_HEIGHT) / 2 + 4;

  return (
    <g className="roadmap-collapsed-node">
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
