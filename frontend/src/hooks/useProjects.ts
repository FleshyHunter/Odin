import { useCallback, useEffect, useRef, useState } from 'react';
import * as projectsApi from '../api/projects';
import type { Project } from '../types';

export function useProjects() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  // See useTracks' initialLoadRef — same fix for the same race: a fast
  // createProject() racing the initial fetch could otherwise either lose
  // the base list (appending to a still-empty `prev`) or get clobbered by
  // it (if the fetch resolves after). Awaiting the same promise on both
  // sides guarantees the initial setProjects(result) always applies first.
  const initialLoadRef = useRef<Promise<Project[]> | null>(null);

  useEffect(() => {
    let cancelled = false;
    const promise = projectsApi.listProjects();
    initialLoadRef.current = promise;
    promise.then((result) => {
      if (cancelled) return;
      setProjects(result);
      setIsLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const createProject = useCallback(async (title: string, description: string | null) => {
    await initialLoadRef.current;
    const project = await projectsApi.createProject(title, description);
    setProjects((prev) => [...prev, project]);
    return project;
  }, []);

  return { projects, isLoading, createProject };
}
