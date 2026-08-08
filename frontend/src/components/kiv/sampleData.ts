import type { KivData } from './types';

// Same Linear Algebra concept set as roadmap/sampleData.ts, for the
// same reason: standalone sample data (no journeys:: backend to fetch
// from yet — deferred.md #38, waiting on a real journeys module before
// real endpoints exist), so the component is real and viewable now.
export const sampleKiv: KivData = {
  trackTitle: 'Linear algebra',
  items: [
    { id: 'vector-addition', conceptTitle: 'Vector addition', severity: 'foundation_gap', masteryScore: 0.42 },
    { id: 'dot-product', conceptTitle: 'Dot product', severity: 'kiv', masteryScore: 0.68 },
    { id: 'norms', conceptTitle: 'Norms', severity: 'kiv', masteryScore: 0.61 },
  ],
};
