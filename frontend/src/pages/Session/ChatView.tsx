import { useEffect, useRef, useState, type DragEvent } from 'react';
import { useNavigate, useOutletContext, useParams } from 'react-router-dom';
import { TrackHeader } from '../../components/conversation/TrackBar/TrackHeader';
import { MessageList } from '../../components/conversation/Messages/MessageList';
import { Composer } from '../../components/conversation/Composer/Composer';
import { DragOverlay } from '../../components/conversation/Composer/DragOverlay/DragOverlay';
import { ActivePanel } from '../../components/active-panel/ActivePanel';
import { MemorylessLanding } from './MemorylessLanding';
import { useJourneyChat } from '../../hooks/useJourneyChat';
import { deleteJourney } from '../../api/journeys';
import { useActivePanel, ACTIVE_PANEL_MIN_WIDTH } from '../../hooks/useActivePanel';
import * as exercisesApi from '../../api/exercises';
import type { Difficulty, Exercise, MasteryStatus } from '../../types';
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
  // deferred.md #2a: real journey-mode chat. Every real Track now always
  // carries a real journeyId (Tracks are created eagerly with their
  // journey together — see api/tracks.ts's createTrackFromJourney) — the
  // old mock-chat fallback for a journeyId-less seed track is gone along
  // with that seed track itself.
  const {
    messages,
    isSending,
    currentConceptId,
    send,
    submitExerciseAnswer,
    cancel,
    composerNotice,
    dismissComposerNotice,
    pendingFiles,
    attachments,
    requestAttach,
    confirmAttachRole,
    cancelPendingFile,
    retryAttachment,
    removeAttachment,
  } = useJourneyChat(activeTrack?.journeyId ?? null);
  // (useJourneyChat still takes journeyId: string | null since it also
  // runs before activeTrack has hydrated, not because journeyId itself
  // is ever null on a real Track.)
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

  // #1/#2: scoped to journeyId + the real currentConceptId now (never
  // trackId — that was the mock's own contract, not what #81's real
  // backend actually needs). Only fires once a journey exists AND its
  // current concept is known — both are null until useJourneyChat's own
  // hydration resolves.
  const journeyId = activeTrack?.journeyId ?? null;
  useEffect(() => {
    if (!journeyId || !currentConceptId) return;
    let cancelled = false;
    exercisesApi.getMasteryStatus(journeyId, currentConceptId, activeTrack?.currentConceptTitle ?? '').then((result) => {
      if (!cancelled) setMastery(result);
    });
    setExercise(null);
    return () => {
      cancelled = true;
    };
  }, [journeyId, currentConceptId, activeTrack?.currentConceptTitle]);

  const handleSubmitAnswer = async (answer: string) => {
    if (!exercise) return undefined;
    const result = await submitExerciseAnswer(exercise.id, answer);
    if (result) {
      setMastery((prev) => (prev ? { ...prev, masteryScore: result.newMastery } : prev));
    }
    return result ?? undefined;
  };

  // deferred.md #94: Map/Roadmap has no real backend wiring yet, so
  // selectedNode.id (the only caller of onStartAttempt) is never a real
  // concept_id — guarded here rather than silently sending a fake id to
  // the real backend. Only actually starts an attempt in the one case
  // that's real right now: the Map happening to be pointed at the SAME
  // concept the tutor is already teaching. Revisit once #94's own real
  // DAG-fetch wiring lands.
  const handleStartAttempt = async (nodeId: string, difficulty: Difficulty) => {
    if (!journeyId || !currentConceptId || nodeId !== currentConceptId) return;
    const result = await exercisesApi.startAttempt(journeyId, currentConceptId, difficulty, activeTrack?.currentConceptTitle ?? '');
    setExercise(result);
  };

  const handleDeleteTrack = () => {
    if (!activeTrackId) return;
    removeTrack(activeTrackId);
    setActiveTrackId('');
  };

  // Independent of handleDeleteTrack above — deletes only the journey
  // (its DAG progress), never this track/conversation. No local state
  // update needed: the track keeps showing exactly as before, since
  // listTracks() never filters on the journey's own deleted_at.
  const handleDeleteJourney = async () => {
    if (!activeTrack) return;
    try {
      await deleteJourney(activeTrack.journeyId);
    } catch (err) {
      window.alert(err instanceof Error ? err.message : 'Failed to delete journey. Try again.');
    }
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

  // deferred.md #37: same depth-counter drag-overlay pattern as
  // MemorylessLanding.tsx — see that file's own comment for why a plain
  // boolean flickers.
  const [isDraggingOver, setIsDraggingOver] = useState(false);
  const dragCounterRef = useRef(0);

  const handleDragEnter = (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    if (!event.dataTransfer.types.includes('Files')) return;
    dragCounterRef.current += 1;
    setIsDraggingOver(true);
  };
  const handleDragOver = (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
  };
  const handleDragLeave = (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    dragCounterRef.current = Math.max(0, dragCounterRef.current - 1);
    if (dragCounterRef.current === 0) setIsDraggingOver(false);
  };
  const handleDrop = (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    dragCounterRef.current = 0;
    setIsDraggingOver(false);
    if (event.dataTransfer.files.length > 0) {
      requestAttach(Array.from(event.dataTransfer.files));
    }
  };
  const dragHandlers = {
    onDragEnter: handleDragEnter,
    onDragOver: handleDragOver,
    onDragLeave: handleDragLeave,
    onDrop: handleDrop,
  };
  const attachProps = {
    attachments,
    pendingFiles,
    onAttachFiles: requestAttach,
    onConfirmAttachRole: confirmAttachRole,
    onCancelPendingFile: cancelPendingFile,
    onRemoveAttachment: removeAttachment,
    onRetryAttachment: retryAttachment,
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
      <main className="conversation" {...dragHandlers}>
        <DragOverlay active={isDraggingOver} />
        <TrackHeader
          title={activeTrack.title}
          conceptTitle={activeTrack.currentConceptTitle}
          isPinned={activeTrack.pinned}
          onPin={handlePin}
          onRename={handleRename}
          onChangeProject={handleChangeProject}
          onRemoveFromProject={handleRemoveFromProject}
          onDelete={handleDeleteTrack}
          onDeleteJourney={handleDeleteJourney}
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
          {...attachProps}
        />
      </main>

      <div
        ref={activePanelShellRef}
        className={isPanelOpen ? 'active-panel-shell active-panel-shell-open' : 'active-panel-shell'}
        style={{ width: isPanelOpen ? panelWidth + 5 : 0 }}
        aria-hidden={!isPanelOpen}
      >
        <div ref={resizeHandleRef} className="active-panel-resize-handle" onMouseDown={handleResizeStart} />
        <ActivePanel
          exercise={exercise}
          mastery={mastery}
          onSubmitAnswer={handleSubmitAnswer}
          onStartAttempt={handleStartAttempt}
          width={panelWidth}
        />
      </div>
    </>
  );
}
