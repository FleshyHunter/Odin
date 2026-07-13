import { useState } from 'react';
import { useNavigate, useOutletContext } from 'react-router-dom';
import { Button } from '../../components/ui/Button/Button';
import { SearchBar } from '../../components/list-view/SearchBar';
import { ListItem } from '../../components/list-view/ListItem';
import type { SessionOutletContext } from '../Session/SessionLayout';
import './tracks.css';

// Browse-all-tracks page — same shape as Projects.tsx (header + "New"
// button, SearchBar, row-list). Reads tracks/createTrack from the layout's
// outlet context rather than calling useTracks() again itself, so this
// page and the Sidebar's Recents/Pinned lists always show the same data.
export function Tracks() {
  const { tracks, createTrack, setActiveTrackId } = useOutletContext<SessionOutletContext>();
  const [search, setSearch] = useState('');
  const navigate = useNavigate();

  const handleNewTrack = async () => {
    const title = window.prompt('What do you want to call this track?');
    if (!title || !title.trim()) return;
    await createTrack(title.trim());
  };

  const handleOpenTrack = (trackId: string) => {
    setActiveTrackId(trackId);
    navigate('/chat');
  };

  const filtered = tracks.filter((track) => track.title.toLowerCase().includes(search.trim().toLowerCase()));

  return (
    <main className="tracks-page">
      <div className="tracks-page-header">
        <h1 className="display">Tracks</h1>
        <Button onClick={handleNewTrack}>+ New track</Button>
      </div>

      <SearchBar value={search} onChange={setSearch} placeholder="Search tracks..." />

      <div className="tracks-list">
        {filtered.length === 0 ? (
          <p className="panel-footnote">
            {tracks.length === 0 ? 'No tracks yet — create one to get started.' : 'No tracks match your search.'}
          </p>
        ) : (
          filtered.map((track) => (
            <ListItem
              key={track.id}
              title={track.title}
              date={track.lastActiveAt}
              description={track.currentConceptTitle}
              onClick={() => handleOpenTrack(track.id)}
            />
          ))
        )}
      </div>
    </main>
  );
}
