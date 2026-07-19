import { Edge } from '../edges/Edge';
import { PulseRing } from '../effects/PulseRing';
import { CollapsedNode } from '../nodes/CollapsedNode';
import { OUTER_R, OUTER_R_JUNCTION, TargetNode } from '../nodes/TargetNode';
import type { RoadmapItem } from '../types';
import { layoutRoadmap } from './layout';

interface RoadmapCanvasProps {
  items: RoadmapItem[];
}

const RING_GAP = 6;

export function RoadmapCanvas({ items }: RoadmapCanvasProps) {
  const { positions, edges, viewBoxWidth, viewBoxHeight } = layoutRoadmap(items);

  return (
    <svg width="100%" height="100%" viewBox={`0 0 ${viewBoxWidth} ${viewBoxHeight}`} role="img" aria-label="Journey map">
      <g>
        {edges.map((edge) => (
          <Edge key={edge.id} x={edge.x} y1={edge.y1} y2={edge.y2} traversed={edge.traversed} />
        ))}
      </g>

      {positions.map(({ item, x, y }) => {
        if (item.kind === 'collapsed') {
          return <CollapsedNode key={item.id} x={x} y={y} lines={item.lines} />;
        }

        const outerR = item.isJunction ? OUTER_R_JUNCTION : OUTER_R;

        return (
          <g key={item.id}>
            {item.status === 'current' && <PulseRing x={x} y={y} radius={outerR + RING_GAP} />}
            <TargetNode x={x} y={y} title={item.title} subtitle={item.subtitle} status={item.status} isJunction={item.isJunction} />
          </g>
        );
      })}
    </svg>
  );
}
