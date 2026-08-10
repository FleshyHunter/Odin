import { useEffect, useRef, useState } from 'react';
import { useNavigate, useOutletContext, useParams } from 'react-router-dom';
import { TrackHeader } from '../../components/conversation/TrackBar/TrackHeader';
import { MessageList } from '../../components/conversation/Messages/MessageList';
import { Composer } from '../../components/conversation/Composer/Composer';
import { ActivePanel } from '../../components/active-panel/ActivePanel';
import { MemorylessLanding } from './MemorylessLanding';
import { useTrackMessages } from '../../hooks/useTracks';
import { useJourneyChat } from '../../hooks/useJourneyChat';
import { useActivePanel, ACTIVE_PANEL_MIN_WIDTH } from '../../hooks/useActivePanel';
import * as exercisesApi from '../../api/exercises';
import type { Exercise, MasteryStatus } from '../../types';
import type { SessionOutletContext } from './SessionLayout';

// The /chat route's content — rendered inside SessionLayout's <Outlet />.
// Shows the active track's conversation, or MemorylessLanding if none is active.
export function ChatView() {
  const {
    activeTrackId,
    activeTrack,
    setActiveTrackId,
    removeTrack,
    togglePin,
    renameTrack,
    setTrackProject,
    openCreateTrackModal,
    openChangeProjectModal,
  } = useOutletContext<SessionOutletContext>();
  const { id: routeId } = useParams<{ id?: string }>();
  const navigate = useNavigate();
  // deferred.md #2a: real journey-mode chat for a track that carries a
  // real journeyId (#4/#40); the mocked hook stays as the fallback for
  // the pre-existing seed track (journeyId: null), which has no real
  // backend to call. Both hooks always run (React's rules of hooks don't
  // allow conditionally calling one or the other) — only the unused
  // one's result is discarded, same pattern already used elsewhere for
  // an optional real-vs-mock split.
  const mockChat = useTrackMessages(activeTrackId);
  const journeyChat = useJourneyChat(activeTrack?.journeyId ?? null);
  // deferred.md #79: `cancel` is now destructured and wired too — both
  // hooks return one (a real AbortController.abort() for journeyChat,
  // a harmless no-op for mockChat, see useTrackMessages's own comment).
  const { messages, isSending, send, cancel, composerNotice, dismissComposerNotice } = activeTrack?.journeyId
    ? journeyChat
    : mockChat;
  const { width: panelWidth, setWidth: setPanelWidth, isOpen: isPanelOpen, toggle: togglePanel } = useActivePanel();
  const resizeHandleRef = useRef<HTMLDivElement>(null);
  const activePanelShellRef = useRef<HTMLDivElement>(null);

  const [exercise, setExercise] = useState<Exercise | null>(null);
  const [mastery, setMastery] = useState<MasteryStatus | null>(null);

  // deferred.md #43: /chat/:id is the one real URL for either a mock
  // track or a real memoryless thread — whichever this id turns out to
  // be. Syncing it into the same activeTrackId state Sidebar/TrackMenu
  // already use means a track match "just works" (tracks.find below
  // resolves it); an id that matches no track correctly leaves
  // activeTrack null, which is exactly what routes to MemorylessLanding.
  useEffect(() => {
    setActiveTrackId(routeId ?? '');
  }, [routeId, setActiveTrackId]);

  const handleThreadCreated = (threadId: string) => {
    navigate(`/chat/${threadId}`, { replace: true });
  };

  useEffect(() => {
    if (!activeTrackId) return;
    let cancelled = false;
    exercisesApi.getCurrentExercise(activeTrackId).then((result) => {
      if (!cancelled) setExercise(result);
    });
    exercisesApi.getMasteryStatus(activeTrackId).then((result) => {
      if (!cancelled) setMastery(result);
    });
    return () => {
      cancelled = true;
    };
  }, [activeTrackId]);

  const handleSubmitAnswer = async (answer: string) => {
    if (!exercise) return;
    const result = await exercisesApi.submitAnswer(exercise.id, answer);
    setMastery((prev) => (prev ? { ...prev, masteryScore: result.masteryScore } : prev));
  };

  const handleDeleteTrack = () => {
    if (!activeTrackId) return;
    removeTrack(activeTrackId);
    setActiveTrackId('');
  };

  const handlePin = () => {
    if (!activeTrackId) return;
    togglePin(activeTrackId);
  };

  const handleRename = () => {
    if (!activeTrackId || !activeTrack) return;
    const title = window.prompt('Rename track', activeTrack.title);
    if (title && title.trim()) renameTrack(activeTrackId, title.trim());
  };

  const handleChangeProject = () => {
    if (!activeTrackId) return;
    openChangeProjectModal(activeTrackId);
  };

  const handleRemoveFromProject = () => {
    if (!activeTrackId) return;
    setTrackProject(activeTrackId, null);
  };

  // Drag-resize: both .conversation and .active-panel have their own
  // min-width in CSS (420px / 320px) — the browser's flex layout already
  // refuses to shrink either past its floor, so this only needs to track
  // the drag delta and let setWidth clamp against the same floor for the
  // persisted value itself (see useActivePanel).
  //
  // .active-panel-shell's own CSS has `transition: width 0.2s ease`,
  // meant for the discrete open/close toggle — but with no guard, that
  // SAME transition was also active during a live drag, and since
  // handleMouseMove below calls setPanelWidth on every mousemove (many
  // times a second), each one retargets the transition mid-flight,
  // fighting itself: visibly glitchy/laggy while dragging wider, and a
  // stray gap on the right while dragging narrower (found live, this
  // session). The `resizing` class disables the transition for exactly
  // the duration of the drag, so width tracks the cursor 1:1 with zero
  // interpolation lag; removing it on mouseup restores the smooth
  // transition for the NEXT discrete open/close toggle, which never
  // fights a rapid-fire state update the way a drag does.
  const handleResizeStart = (event: React.MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = panelWidth;
    resizeHandleRef.current?.classList.add('resizing');
    activePanelShellRef.current?.classList.add('resizing');
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';

    const handleMouseMove = (moveEvent: MouseEvent) => {
      // .active-panel sits to the right of .conversation — dragging left
      // (smaller clientX) should widen the panel, so the delta is inverted.
      const delta = startX - moveEvent.clientX;
      setPanelWidth(Math.max(ACTIVE_PANEL_MIN_WIDTH, startWidth + delta));
    };
    const handleMouseUp = () => {
      resizeHandleRef.current?.classList.remove('resizing');
      activePanelShellRef.current?.classList.remove('resizing');
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
  };

  if (!activeTrack) {
    // routeId itself (not activeTrackId) is the real signal here: once a
    // brand new thread's id is learned and the URL updates to /chat/:id,
    // routeId briefly lags activeTrackId's own state update by a render
    // (see handleThreadCreated) — passing routeId keeps this prop in
    // exact lockstep with what's actually in the address bar.
    return (
      <MemorylessLanding
        onStartTrack={openCreateTrackModal}
        threadId={routeId ?? null}
        onThreadCreated={handleThreadCreated}
        onConvert={openCreateTrackModal}
      />
    );
  }

  return (
    <>
      <main className="conversation">
        <TrackHeader
          title={activeTrack.title}
          conceptTitle={activeTrack.currentConceptTitle}
          isPinned={activeTrack.pinned}
          onPin={handlePin}
          onRename={handleRename}
          onChangeProject={handleChangeProject}
          onRemoveFromProject={handleRemoveFromProject}
          onDelete={handleDeleteTrack}
          isPanelOpen={isPanelOpen}
          onTogglePanel={togglePanel}
        />
        <MessageList messages={messages} onRetry={send} />
        <Composer
          onSend={send}
          isSending={isSending}
          onStop={cancel}
          notice={composerNotice}
          onDismissNotice={dismissComposerNotice}
        />
      </main>

      <div
        ref={activePanelShellRef}
        className={isPanelOpen ? 'active-panel-shell active-panel-shell-open' : 'active-panel-shell'}
        style={{ width: isPanelOpen ? panelWidth + 5 : 0 }}
        aria-hidden={!isPanelOpen}
      >
        <div ref={resizeHandleRef} className="active-panel-resize-handle" onMouseDown={handleResizeStart} />
        <ActivePanel exercise={exercise} mastery={mastery} onSubmitAnswer={handleSubmitAnswer} width={panelWidth} />
      </div>
    </>
  );
}
