import { useNavigate, useOutletContext, useParams } from 'react-router-dom';
import { useProjects } from '../../hooks/useProjects';
import { ListItem } from '../../components/list-view/ListItem';
import type { SessionOutletContext } from '../Session/SessionLayout';
import './projectDetail.css';

// /projects/:id (deferred.md #50) — the index/lens view DESIGN.md's
// Routing table describes: lists the tracks currently inside this
// project. Clicking a track navigates straight to /chat/:id, never a
// combined /projects/:id/:trackId path — DESIGN.md is explicit this was
// considered and rejected (a project-prefixed track address breaks the
// same way /tracks-prefixed ones would the moment "Change project" runs).
export function ProjectDetail() {
  const { id } = useParams<{ id: string }>();
  const { projects } = useProjects();
  const { tracks } = useOutletContext<SessionOutletContext>();
  const navigate = useNavigate();

  const project = projects.find((p) => p.id === id) ?? null;
  const projectTracks = tracks.filter((track) => track.projectId === id);

  if (!project) {
    return (
      <main className="project-detail-page">
        <p className="panel-footnote">This project doesn't exist or was removed.</p>
      </main>
    );
  }

  return (
    <main className="project-detail-page">
      <div className="project-detail-header">
        <h1 className="display">{project.title}</h1>
        {project.description && <p className="project-detail-description">{project.description}</p>}
      </div>

      <div className="project-detail-list">
        {projectTracks.length === 0 ? (
          <p className="panel-footnote">
            No tracks in this project yet — use a track's "Change project" menu to add one.
          </p>
        ) : (
          projectTracks.map((track) => (
            <ListItem
              key={track.id}
              title={track.title}
              date={track.lastActiveAt}
              description={track.currentConceptTitle}
              onClick={() => navigate(`/chat/${track.id}`)}
            />
          ))
        )}
      </div>
    </main>
  );
}
