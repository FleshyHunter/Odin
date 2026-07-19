import type { ConceptStatus } from '../types';
import './nodes.css';

interface TargetNodeProps {
  x: number;
  y: number;
  title: string;
  subtitle?: string;
  status: ConceptStatus;
  isJunction?: boolean;
}

// Junction nodes are sized larger rather than drawing merge lines —
// borrowed from transit-map interchange-station convention, per the
// mockup's own comment (odin_map_view.html).
const OUTER_R = 10;
const OUTER_R_JUNCTION = 14;
const INNER_R = 7;
const INNER_R_JUNCTION = 10;
const LABEL_GAP = 6;

export function TargetNode({ x, y, title, subtitle, status, isJunction }: TargetNodeProps) {
  const outerR = isJunction ? OUTER_R_JUNCTION : OUTER_R;
  const innerR = isJunction ? INNER_R_JUNCTION : INNER_R;
  const lit = status === 'mastered' || status === 'current';
  const labelX = x + outerR + LABEL_GAP;
  const titleY = subtitle ? y - 4 : y + 4;
  const subtitleY = y + 8;

  return (
    <g className={`roadmap-target-node ${lit ? 'lit' : 'dim'}`}>
      <circle cx={x} cy={y} r={outerR} className="roadmap-target-outer" />
      <circle cx={x} cy={y} r={innerR} className="roadmap-target-inner" />
      <text x={labelX} y={titleY} className={`roadmap-node-title ${status === 'current' ? 'current' : ''}`}>
        {title}
      </text>
      {subtitle && (
        <text x={labelX} y={subtitleY} className={`roadmap-node-subtitle ${status === 'current' ? 'current' : ''}`}>
          {subtitle}
        </text>
      )}
    </g>
  );
}

export { OUTER_R, OUTER_R_JUNCTION };
