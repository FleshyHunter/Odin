import type { Project } from '../types';
import { simulateDelay } from './client';

// In-memory only — same pattern as api/tracks.ts's track store. Normally
// starts empty (true empty state — no projects until the user creates
// one); one demo entry is seeded below at the user's explicit request, to
// have something on screen while designing/reviewing the Projects and
// Tracks list pages. Remove this entry to go back to a true empty start.
const projects: Project[] = [
  {
    id: 'project-seed-1',
    title: 'Machine Learning Research',
    description: 'Understand papers on action recognition and build models that work with small training sets',
    updatedAt: new Date().toISOString(),
  },
];

// Real contract: GET /projects -> Project[]
export async function listProjects(): Promise<Project[]> {
  return simulateDelay([...projects]);
}

// Real contract: POST /projects { title } -> Project
// description stays null — there's no creation flow that collects one
// (matches createTrack's title-only prompt() stand-in).
export async function createProject(title: string): Promise<Project> {
  const project: Project = {
    id: `project-${Date.now()}`,
    title,
    description: null,
    updatedAt: new Date().toISOString(),
  };
  projects.push(project);
  return simulateDelay(project, 300);
}
