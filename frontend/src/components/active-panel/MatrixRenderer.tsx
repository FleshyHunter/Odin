import type { MatrixValue } from '../../types';

interface MatrixRendererProps {
  matrix: MatrixValue;
}

// Renders matrix notation via CSS grid + serif brackets — not an image,
// not LaTeX — exactly as in odin_session.html. The prototype hardcodes a
// 2-column grid for its one example matrix; here gridTemplateColumns is
// driven by matrix.cols so the component generalizes to other sizes while
// rendering identically for the 2x2 case shown in the prototype.
export function MatrixRenderer({ matrix }: MatrixRendererProps) {
  return (
    <div className="matrix">
      <span className="bracket">[</span>
      <div className="matrix-grid" style={{ gridTemplateColumns: `repeat(${matrix.cols}, 1fr)` }}>
        {matrix.values.map((value, index) => (
          <span key={index}>{value}</span>
        ))}
      </div>
      <span className="bracket">]</span>
    </div>
  );
}
