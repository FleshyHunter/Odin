import { useEffect, useState } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';
import { Sidebar, type SidebarSection } from '../../components/sidebar/Sidebar/Sidebar';
import { useTracks } from '../../hooks/useTracks';
import { useProjects } from '../../hooks/useProjects';
import { TrackModal } from '../../components/tracks/TrackModal/TrackModal';
import { ChangeProjectModal } from '../../components/tracks/ChangeProjectModal/ChangeProjectModal';
import { Modal } from '../../components/ui/Modal/Modal';
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
  // deferred.md #73 — set only when guardedNavigate below intercepts a
  // real navigation away from a live, unconverted memoryless thread.
  // `proceed` is the original navigation, deferred until the student
  // actually confirms leaving.
  const [pendingLeave, setPendingLeave] = useState<{ threadId: string; proceed: () => void } | null>(null);
  const location = useLocation();
  const navigate = useNavigate();
  const activeTrack = tracks.find((track) => track.id === activeTrackId) ?? null;

  // "Tracks" nav now mirrors "Projects" exactly: both point at their own
  // browse-all page. /chat (bare) is reached by opening a track from any
  // list, not by clicking "Tracks" — so it isn't part of this check.
  const urlSection: SidebarSection = location.pathname.startsWith('/projects') ? 'projects' : 'tracks';
  const activeSection: SidebarSection = pinnedActive ? 'pinned' : urlSection;

  // /chat/:id is either a real track or a real memoryless thread (see
  // ChatView's own comment) — activeTrack stays null for the latter, so
  // "an id is present in the URL AND it matches no known track" IS
  // exactly "currently viewing a live memoryless conversation." Derived
  // straight from state SessionLayout already has — no new prop
  // threading from MemorylessLanding needed.
  const currentMemorylessThreadId = (() => {
    const match = location.pathname.match(/^\/chat\/([^/]+)$/);
    return match && activeTrack === null ? match[1] : null;
  })();

  // Wraps every real navigate()-away action (clicking a different track,
  // New chat, Tracks/Projects) — if leaving right now would abandon a
  // live memoryless thread, the action is held until the student
  // confirms via the modal below instead of running immediately.
  const guardedNavigate = (action: () => void) => {
    if (currentMemorylessThreadId) {
      setPendingLeave({ threadId: currentMemorylessThreadId, proceed: action });
    } else {
      action();
    }
  };

  // Sign-out is a leave-the-app action same as any guarded navigate() —
  // App.tsx's handleSignOut previously bypassed this guard entirely
  // (its own separate useNavigate() instance has no access to
  // currentMemorylessThreadId/pendingLeave, both local to this
  // component), so signing out while on a live memoryless thread never
  // warned. Wrapping locally here, rather than lifting this guard's
  // state up into App.tsx, keeps the fix to one call site — App.tsx's
  // onSignOut stays exactly "do the real sign-out," this component
  // stays the one place that decides whether to warn first.
  const guardedSignOut = () => guardedNavigate(onSignOut);

  // Browser-level leave (hard refresh, tab close, closing the window) —
  // deferred.md #73 flagged this as consciously scoped out, not built.
  // The browser only ever shows its own generic "leave site?" copy
  // (no custom text is possible for beforeunload, by design across
  // every browser), so this just needs to arm/disarm the prompt based
  // on the same "is there a live memoryless thread" check guardedNavigate
  // already uses — no separate state to track.
  useEffect(() => {
    if (!currentMemorylessThreadId) return;
    const handleBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = '';
    };
    window.addEventListener('beforeunload', handleBeforeUnload);
    return () => window.removeEventListener('beforeunload', handleBeforeUnload);
  }, [currentMemorylessThreadId]);

  const handleSectionChange = (section: SidebarSection) => {
    // Doesn't navigate at all (RecentsList's own filter, same route) —
    // nothing to guard.
    if (section === 'pinned') {
      setPinnedActive(true);
      return;
    }
    guardedNavigate(() => {
      setPinnedActive(false);
      navigate(section === 'projects' ? '/projects' : '/tracks');
    });
  };

  const handleSelectTrack = (trackId: string) => {
    guardedNavigate(() => {
      setPinnedActive(false);
      navigate(`/chat/${trackId}`);
      setActiveTrackId(trackId);
    });
  };

  const handleHome = () => {
    guardedNavigate(() => {
      setPinnedActive(false);
      setActiveTrackId('');
      navigate('/chat');
    });
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
          onSignOut={guardedSignOut}
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

      {/* deferred.md #73 — the accurate framing, not "your data will be
          lost": write-through already persists messages durably; the
          real problem is there's no route back to this thread once you
          leave (it never appears in Recents, and GET /memoryless/
          threads/:id only reads Redis, which eventually expires). */}
      <Modal open={pendingLeave !== null} title="Leave this chat?" onClose={() => setPendingLeave(null)}>
        <p className="modal-hint">
          This chat isn't part of a track yet — once you leave, you won't be able to get back to it.
        </p>
        <div className="modal-actions">
          <button type="button" className="btn-primary" onClick={() => setPendingLeave(null)}>
            Stay
          </button>
          <button
            type="button"
            className="btn-secondary"
            onClick={() => {
              const threadId = pendingLeave?.threadId;
              setPendingLeave(null);
              if (threadId) openCreateTrackModal(threadId);
            }}
          >
            Add to a track
          </button>
          <button
            type="button"
            className="btn-secondary"
            onClick={() => {
              pendingLeave?.proceed();
              setPendingLeave(null);
            }}
          >
            Leave anyway
          </button>
        </div>
      </Modal>
    </div>
  );
}
