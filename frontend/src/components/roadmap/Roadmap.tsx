import { Legend } from './Legend';
import { RoadmapCanvas } from './flow/RoadmapCanvas';
import './roadmap.css';
import { sampleRoadmap } from './sampleData';
import type { RoadmapData } from './types';

interface RoadmapProps {
  data?: RoadmapData;
}

// Public entry point for the Map tab. No trackId/data wiring yet —
// that's separate follow-up work — so this renders standalone sample
// data by default until a real fetch is threaded in from ChatView.
export function Roadmap({ data = sampleRoadmap }: RoadmapProps) {
  return (
    <div className="roadmap">
      <div className="roadmap-header">
        <h3 className="display">{data.trackTitle}</h3>
        <span className="roadmap-progress">
          {data.masteredCount} of {data.totalCount} mastered
        </span>
      </div>

      <div className="roadmap-canvas-wrap">
        <RoadmapCanvas items={data.items} />
      </div>

      <Legend />
    </div>
  );
}
