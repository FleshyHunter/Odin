import './edges.css';

interface EdgeProps {
  x: number;
  y1: number;
  y2: number;
  traversed: boolean;
}

// Straight vertical segments only — no branch-merge lines drawn.
// Convergence is handled by navigation (work the other required
// branch, then return here), not by rendering two lines meeting.
export function Edge({ x, y1, y2, traversed }: EdgeProps) {
  return <line x1={x} y1={y1} x2={x} y2={y2} className={traversed ? 'roadmap-edge traversed' : 'roadmap-edge'} />;
}
