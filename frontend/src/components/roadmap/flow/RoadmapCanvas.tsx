import { useState } from 'react';
import { Edge } from '../edges/Edge';
import { PulseRing } from '../effects/PulseRing';
import { CollapsedNode } from '../nodes/CollapsedNode';
import { OUTER_R, OUTER_R_JUNCTION, TargetNode } from '../nodes/TargetNode';
import type { RoadmapItem } from '../types';
import { layoutRoadmap } from './layout';

interface RoadmapCanvasProps {
  items: RoadmapItem[];
  onNodeClick?: (id: string, title: string) => void;
}

const RING_GAP = 6;

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
    <svg width="100%" height="100%" viewBox={`0 0 ${viewBoxWidth} ${viewBoxHeight}`} role="img" aria-label="Journey map">
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
              onClick={onNodeClick ? () => onNodeClick(item.id, item.title) : undefined}
            />
          </g>
        );
      })}
    </svg>
  );
}
