import type { Project } from '../types';
import { simulateDelay } from './client';

// In-memory only — same pattern as api/tracks.ts's track store. Starts
// empty: no projects exist until the user actually creates one.
const projects: Project[] = [];

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
