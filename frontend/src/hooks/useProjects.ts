import { useCallback, useEffect, useState } from 'react';
import * as projectsApi from '../api/projects';
import type { Project } from '../types';

export function useProjects() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    projectsApi.listProjects().then((result) => {
      if (cancelled) return;
      setProjects(result);
      setIsLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const createProject = useCallback(async (title: string) => {
    const project = await projectsApi.createProject(title);
    setProjects((prev) => [...prev, project]);
    return project;
  }, []);

  return { projects, isLoading, createProject };
}
