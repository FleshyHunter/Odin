import type { RoadmapData } from './types';

// Mirrors prompt sequence/odin_map_view.html's concept chain 1:1 — this
// is standalone sample data (no track/backend wiring yet), so the
// component is real and viewable without needing the data-fetching work
// that's explicitly out of scope for this pass.
export const sampleRoadmap: RoadmapData = {
  trackTitle: 'Linear algebra',
  masteredCount: 5,
  totalCount: 11,
  items: [
    { kind: 'node', id: 'vectors', title: 'Vectors', status: 'mastered' },
    { kind: 'node', id: 'matrices', title: 'Matrices', status: 'mastered' },
    {
      kind: 'node',
      id: 'matrix-ops',
      title: 'Matrix ops',
      status: 'current',
      subtitle: 'you are here',
    },
    {
      kind: 'node',
      id: 'eigenvalues',
      title: 'Eigenvalues',
      status: 'pending',
      isJunction: true,
      subtitle: '+ Vector spaces',
    },
    { kind: 'collapsed', id: 'foundational-chain', lines: ['5 foundational', 'concepts'] },
    { kind: 'node', id: 'diagonalization', title: 'Diagonalization', status: 'pending' },
  ],
};
