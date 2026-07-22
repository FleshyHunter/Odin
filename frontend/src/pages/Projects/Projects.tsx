import { useState } from 'react';
import { useProjects } from '../../hooks/useProjects';
import { Button } from '../../components/ui/Button/Button';
import { SearchBar } from '../../components/list-view/SearchBar';
import { ListItem } from '../../components/list-view/ListItem';
import { ProjectModal } from '../../components/projects/ProjectModal/ProjectModal';
import './projects.css';

// Claude's pattern for the index/discovery layer (dedicated page, search
// bar, "New project" creation) + GPT's pattern for how items are listed
// (title + date row, optional description, divider between rows —
// row-list, not a card grid).
export function Projects() {
  const { projects, createProject } = useProjects();
  const [search, setSearch] = useState('');
  const [isCreateOpen, setIsCreateOpen] = useState(false);

  const filtered = projects.filter((project) =>
    project.title.toLowerCase().includes(search.trim().toLowerCase()),
  );

  return (
    <>
      <main className="projects-page">
        <div className="projects-page-header">
          <h1 className="display">Projects</h1>
          <Button onClick={() => setIsCreateOpen(true)}>+ New project</Button>
        </div>

        <SearchBar value={search} onChange={setSearch} placeholder="Search projects..." />

        <div className="projects-list">
          {filtered.length === 0 ? (
            <p className="panel-footnote">
              {projects.length === 0
                ? 'No projects yet — create one to get started.'
                : 'No projects match your search.'}
            </p>
          ) : (
            filtered.map((project) => (
              <ListItem
                key={project.id}
                title={project.title}
                date={project.updatedAt}
                description={project.description}
              />
            ))
          )}
        </div>
      </main>

      <ProjectModal
        open={isCreateOpen}
        onClose={() => setIsCreateOpen(false)}
        onCreate={createProject}
      />
    </>
  );
}
