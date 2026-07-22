import { useState } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';
import { Sidebar, type SidebarSection } from '../../components/sidebar/Sidebar/Sidebar';
import { useTracks } from '../../hooks/useTracks';
import { TrackModal } from '../../components/tracks/TrackModal/TrackModal';
import type { Track } from '../../types';
import './Session.css';

interface SessionLayoutProps {
  profileName: string;
}

export interface SessionOutletContext {
  tracks: Track[];
  activeTrackId: string;
  activeTrack: Track | null;
  setActiveTrackId: (id: string) => void;
  createTrack: (title: string) => Promise<Track>;
  removeTrack: (id: string) => Promise<void>;
  togglePin: (id: string) => Promise<void>;
  openCreateTrackModal: () => void;
}

// Layout route for /chat, /projects and /tracks (see App.tsx) — mirrors
// AuthLayout's pattern: this (and the Sidebar it renders) stays mounted
// across navigation between them; only the matched child swaps via
// <Outlet />. tracks/createTrack are passed through the outlet context
// (not just used locally for the Sidebar) so the Tracks browse page reads
// and writes the exact same list the Sidebar does, rather than each
// holding its own separate useTracks() copy that could drift out of sync.
// Section highlighting is derived from the URL itself now (real routes),
// not local state — except "Pinned", which still has no route/content of
// its own (out of scope so far), so it stays a harmless local-only toggle.
export function SessionLayout({ profileName }: SessionLayoutProps) {
  const { tracks, removeTrack, createTrack, togglePin } = useTracks();
  const [activeTrackId, setActiveTrackId] = useState('');
  const [pinnedActive, setPinnedActive] = useState(false);
  const [isTrackModalOpen, setIsTrackModalOpen] = useState(false);
  const location = useLocation();
  const navigate = useNavigate();

  // "Tracks" nav now mirrors "Projects" exactly: both point at their own
  // browse-all page. /chat (bare) is reached by opening a track from any
  // list, not by clicking "Tracks" — so it isn't part of this check.
  const urlSection: SidebarSection = location.pathname.startsWith('/projects') ? 'projects' : 'tracks';
  const activeSection: SidebarSection = pinnedActive ? 'pinned' : urlSection;

  const handleSectionChange = (section: SidebarSection) => {
    if (section === 'pinned') {
      setPinnedActive(true);
      return;
    }
    setPinnedActive(false);
    navigate(section === 'projects' ? '/projects' : '/tracks');
  };

  const handleSelectTrack = (trackId: string) => {
    setPinnedActive(false);
    navigate('/chat');
    setActiveTrackId(trackId);
  };

  const handleHome = () => {
    setPinnedActive(false);
    setActiveTrackId('');
    navigate('/chat');
  };

  const openCreateTrackModal = () => setIsTrackModalOpen(true);

  const handleCreateTrack = async (title: string) => {
    const track = await createTrack(title);
    setPinnedActive(false);
    navigate('/chat');
    setActiveTrackId(track.id);
  };

  const activeTrack = tracks.find((track) => track.id === activeTrackId) ?? null;

  return (
    <div className="session-page">
      {/* .app-shell is a flex row now (see Session.css) — it naturally
          reflows around however many children the Outlet renders (2 for
          Projects/Tracks/EmptyLanding, 3 for an open conversation's
          conversation+active-panel), so no separate "empty" variant
          class is needed here anymore. */}
      <div className="app-shell">
        <Sidebar
          tracks={tracks}
          activeTrackId={activeTrackId}
          onSelectTrack={handleSelectTrack}
          onNewTrack={openCreateTrackModal}
          profileName={profileName}
          activeSection={activeSection}
          onSectionChange={handleSectionChange}
          onHome={handleHome}
        />

        <Outlet
          context={
            {
              tracks,
              activeTrackId,
              activeTrack,
              setActiveTrackId,
              createTrack,
              removeTrack,
              togglePin,
              openCreateTrackModal,
            } satisfies SessionOutletContext
          }
        />
      </div>

      <TrackModal
        open={isTrackModalOpen}
        onClose={() => setIsTrackModalOpen(false)}
        onCreate={handleCreateTrack}
      />
    </div>
  );
}
