import { useState } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';
import { Sidebar, type SidebarSection } from '../../components/sidebar/Sidebar/Sidebar';
import { useTracks } from '../../hooks/useTracks';
import { useProjects } from '../../hooks/useProjects';
import { TrackModal } from '../../components/tracks/TrackModal/TrackModal';
import { ChangeProjectModal } from '../../components/tracks/ChangeProjectModal/ChangeProjectModal';
import type { Track } from '../../types';
import './Session.css';

interface SessionLayoutProps {
  profileName: string;
  email: string;
  onSignOut: () => void;
}

export interface SessionOutletContext {
  tracks: Track[];
  activeTrackId: string;
  activeTrack: Track | null;
  setActiveTrackId: (id: string) => void;
  createTrackFromJourney: (title: string, journeyId: string) => Promise<Track>;
  removeTrack: (id: string) => Promise<void>;
  togglePin: (id: string) => Promise<void>;
  renameTrack: (id: string, title: string) => Promise<void>;
  setTrackProject: (id: string, projectId: string | null) => Promise<void>;
  // deferred.md #8: an optional thread_id seeds TrackModal so, once a
  // real journey exists, it can convert that memoryless thread into it
  // (deferred.md #17) before calling onJourneyCreated. Callers that wire
  // this directly to a raw DOM event handler (a plain onClick, not an
  // explicit `() => openCreateTrackModal()` wrapper) MUST NOT do so —
  // the event object would land in this param instead of undefined.
  openCreateTrackModal: (initialThreadId?: string) => void;
  openChangeProjectModal: (trackId: string) => void;
}

// Layout route for /chat, /projects and /tracks (see App.tsx) — mirrors
// AuthLayout's pattern: this (and the Sidebar it renders) stays mounted
// across navigation between them; only the matched child swaps via
// <Outlet />. tracks/createTrackFromJourney are passed through the outlet context
// (not just used locally for the Sidebar) so the Tracks browse page reads
// and writes the exact same list the Sidebar does, rather than each
// holding its own separate useTracks() copy that could drift out of sync.
// Section highlighting is derived from the URL itself now (real routes),
// not local state — except "Pinned", which still has no route/content of
// its own (out of scope so far), so it stays a harmless local-only toggle.
export function SessionLayout({ profileName, email, onSignOut }: SessionLayoutProps) {
  const { tracks, removeTrack, createTrackFromJourney, togglePin, renameTrack, setTrackProject } = useTracks();
  const { projects } = useProjects();
  const [activeTrackId, setActiveTrackId] = useState('');
  const [pinnedActive, setPinnedActive] = useState(false);
  const [isTrackModalOpen, setIsTrackModalOpen] = useState(false);
  const [trackModalThreadId, setTrackModalThreadId] = useState<string | null>(null);
  const [changeProjectTrackId, setChangeProjectTrackId] = useState<string | null>(null);
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
    navigate(`/chat/${trackId}`);
    setActiveTrackId(trackId);
  };

  const handleHome = () => {
    setPinnedActive(false);
    setActiveTrackId('');
    navigate('/chat');
  };

  const openCreateTrackModal = (initialThreadId?: string) => {
    setTrackModalThreadId(initialThreadId ?? null);
    setIsTrackModalOpen(true);
  };
  const openChangeProjectModal = (trackId: string) => setChangeProjectTrackId(trackId);
  const changeProjectTrack = tracks.find((track) => track.id === changeProjectTrackId) ?? null;

  const handleJourneyCreated = async (title: string, journeyId: string) => {
    const track = await createTrackFromJourney(title, journeyId);
    setPinnedActive(false);
    navigate(`/chat/${track.id}`);
    setActiveTrackId(track.id);
  };

  const activeTrack = tracks.find((track) => track.id === activeTrackId) ?? null;

  return (
    <div className="session-page">
      {/* .app-shell is a flex row now (see Session.css) — it naturally
          reflows around however many children the Outlet renders (2 for
          Projects/Tracks/MemorylessLanding, 3 for an open conversation's
          conversation+active-panel), so no separate "empty" variant
          class is needed here anymore. */}
      <div className="app-shell">
        <Sidebar
          tracks={tracks}
          activeTrackId={activeTrackId}
          onSelectTrack={handleSelectTrack}
          onNewChat={handleHome}
          onNewTrack={() => openCreateTrackModal()}
          profileName={profileName}
          email={email}
          onSignOut={onSignOut}
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
              createTrackFromJourney,
              removeTrack,
              togglePin,
              renameTrack,
              setTrackProject,
              openCreateTrackModal,
              openChangeProjectModal,
            } satisfies SessionOutletContext
          }
        />
      </div>

      <TrackModal
        open={isTrackModalOpen}
        onClose={() => {
          setIsTrackModalOpen(false);
          setTrackModalThreadId(null);
        }}
        initialThreadId={trackModalThreadId}
        onJourneyCreated={handleJourneyCreated}
      />

      <ChangeProjectModal
        open={changeProjectTrackId !== null}
        onClose={() => setChangeProjectTrackId(null)}
        projects={projects}
        currentProjectId={changeProjectTrack?.projectId ?? null}
        onSelect={(projectId) => setTrackProject(changeProjectTrackId as string, projectId)}
      />
    </div>
  );
}
