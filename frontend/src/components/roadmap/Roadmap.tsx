import { Legend } from './Legend';
import { RoadmapCanvas } from './flow/RoadmapCanvas';
import './roadmap.css';
import type { ConceptStatus, RoadmapData } from './types';

interface RoadmapProps {
  data: RoadmapData;
  onNodeClick?: (id: string, title: string, status: ConceptStatus, unmetPrerequisiteTitles: string[]) => void;
}

// Public entry point for the Map tab — deferred.md #94: ActivePanel.tsx
// owns the real fetch and only ever renders this once it resolves (a
// loading placeholder shows until then), so `data` is always real here,
// never sample/placeholder.
export function Roadmap({ data, onNodeClick }: RoadmapProps) {
  return (
    <div className="roadmap">
      <div className="roadmap-header">
        <h3 className="display">{data.trackTitle}</h3>
        <span className="roadmap-progress">
          {data.masteredCount} of {data.totalCount} mastered
        </span>
      </div>

      <div className="roadmap-canvas-wrap">
        <RoadmapCanvas items={data.items} onNodeClick={onNodeClick} />
      </div>

      <Legend />
    </div>
  );
}
