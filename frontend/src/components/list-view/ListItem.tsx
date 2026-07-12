import type { Project } from '../../types';
import './listItem.css';

interface ListItemProps {
  project: Project;
  onClick?: () => void;
}

function formatUpdatedAt(iso: string): string {
  return new Date(iso).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

export function ListItem({ project, onClick }: ListItemProps) {
  return (
    <div className="project-list-item" role="button" tabIndex={0} onClick={onClick}>
      <div className="project-list-item-row">
        <span className="project-list-item-title">{project.title}</span>
        <span className="project-list-item-date">{formatUpdatedAt(project.updatedAt)}</span>
      </div>
      {project.description && <p className="project-list-item-description">{project.description}</p>}
    </div>
  );
}
