export function Legend() {
  return (
    <div className="roadmap-legend">
      <div className="roadmap-legend-item">
        <span className="roadmap-legend-dot mastered" /> Mastered
      </div>
      <div className="roadmap-legend-item">
        <span className="roadmap-legend-dot current" /> Current
      </div>
      <div className="roadmap-legend-item">
        <span className="roadmap-legend-dot pending" /> Not yet reached
      </div>
      <div className="roadmap-legend-item">
        <span className="roadmap-legend-swatch-collapsed" /> Collapsed chain (tap to expand)
      </div>
    </div>
  );
}
