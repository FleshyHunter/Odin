import { useState } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';
import { Sidebar, type SidebarSection } from '../../components/sidebar/Sidebar/Sidebar';
import { useTracks } from '../../hooks/useTracks';
import type { Track } from '../../types';
import './Session.css';

interface SessionLayoutProps {
  profileName: string;
}

export interface SessionOutletContext {
  activeTrackId: string;
  activeTrack: Track | null;
  setActiveTrackId: (id: string) => void;
  removeTrack: (id: string) => Promise<void>;
  togglePin: (id: string) => Promise<void>;
}

// Layout route for /chat and /project (see App.tsx) — mirrors AuthLayout's
// pattern: this (and the Sidebar it renders) stays mounted across
// navigation between the two; only the matched child swaps via <Outlet />.
// Section highlighting is derived from the URL itself now (real routes),
// not local state — except "Pinned", which still has no route/content of
// its own (out of scope so far), so it stays a harmless local-only toggle.
export function SessionLayout({ profileName }: SessionLayoutProps) {
  const { tracks, removeTrack, createTrack, togglePin } = useTracks();
  const [activeTrackId, setActiveTrackId] = useState('');
  const [pinnedActive, setPinnedActive] = useState(false);
  const location = useLocation();
  const navigate = useNavigate();

  const urlSection: SidebarSection = location.pathname.startsWith('/project') ? 'projects' : 'tracks';
  const activeSection: SidebarSection = pinnedActive ? 'pinned' : urlSection;

  const handleSectionChange = (section: SidebarSection) => {
    if (section === 'pinned') {
      setPinnedActive(true);
      return;
    }
    setPinnedActive(false);
    navigate(section === 'projects' ? '/project' : '/chat');
  };

  const handleSelectTrack = (trackId: string) => {
    setPinnedActive(false);
    navigate('/chat');
    setActiveTrackId(trackId);
  };

  const handleNewTrack = async () => {
    // No "create track" design exists yet (no prototype markup, no
    // ARCHITECTURE_LOCK.md flow for it) — a native prompt() is the same
    // minimal stand-in already used for TrackMenu's delete confirm(),
    // not a designed modal.
    const title = window.prompt('What do you want to call this track?');
    if (!title || !title.trim()) return;
    const track = await createTrack(title.trim());
    setPinnedActive(false);
    navigate('/chat');
    setActiveTrackId(track.id);
  };

  const activeTrack = tracks.find((track) => track.id === activeTrackId) ?? null;
  const showConversation = urlSection === 'tracks' && !!activeTrack;

  return (
    <div className="session-page">
      <div className={showConversation ? 'app-shell' : 'app-shell app-shell-empty'}>
        <Sidebar
          tracks={tracks}
          activeTrackId={activeTrackId}
          onSelectTrack={handleSelectTrack}
          onNewTrack={handleNewTrack}
          profileName={profileName}
          activeSection={activeSection}
          onSectionChange={handleSectionChange}
        />

        <Outlet
          context={
            { activeTrackId, activeTrack, setActiveTrackId, removeTrack, togglePin } satisfies SessionOutletContext
          }
        />
      </div>
    </div>
  );
}
