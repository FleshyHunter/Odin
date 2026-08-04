import { useState } from 'react';
import { Modal } from '../../ui/Modal/Modal';
import type { Project } from '../../../types';

interface ChangeProjectModalProps {
  open: boolean;
  onClose: () => void;
  projects: Project[];
  currentProjectId: string | null;
  onSelect: (projectId: string) => Promise<unknown>;
}

// deferred.md #41 — TrackMenu's "Change project" was wired to nothing;
// this is the picker that gives it something to call. "Remove from
// project" is a separate, direct TrackMenu action (no picker needed to
// clear a value), not offered as a choice inside this modal.
export function ChangeProjectModal({ open, onClose, projects, currentProjectId, onSelect }: ChangeProjectModalProps) {
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSelect = async (projectId: string) => {
    if (isSubmitting || projectId === currentProjectId) return;
    setIsSubmitting(true);
    try {
      await onSelect(projectId);
      onClose();
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Modal open={open} title="Change project" onClose={onClose}>
      {projects.length === 0 ? (
        <p className="modal-hint">No projects yet — create one first from the Projects page.</p>
      ) : (
        <div className="modal-option-list">
          {projects.map((project) => (
            <button
              key={project.id}
              type="button"
              className="modal-option"
              aria-pressed={project.id === currentProjectId}
              disabled={isSubmitting}
              onClick={() => handleSelect(project.id)}
            >
              <span>{project.title}</span>
              {project.id === currentProjectId && <span className="modal-option-current">Current</span>}
            </button>
          ))}
        </div>
      )}
    </Modal>
  );
}
