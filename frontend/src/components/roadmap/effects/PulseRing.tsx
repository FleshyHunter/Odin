import './effects.css';

interface PulseRingProps {
  x: number;
  y: number;
  radius: number;
}

// CSS-animated, not requestAnimationFrame (unlike BigDipperCanvas) —
// global.css already has a blanket prefers-reduced-motion rule that
// zeroes animation durations, so this gets that handling for free
// instead of needing a manual matchMedia check.
export function PulseRing({ x, y, radius }: PulseRingProps) {
  return <circle cx={x} cy={y} r={radius} className="roadmap-pulse-ring" />;
}
