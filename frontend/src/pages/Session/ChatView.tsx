import { useEffect, useRef, useState } from 'react';
import { useOutletContext } from 'react-router-dom';
import { TrackHeader } from '../../components/conversation/TrackBar/TrackHeader';
import { MessageList } from '../../components/conversation/Messages/MessageList';
import { Composer } from '../../components/conversation/Composer/Composer';
import { ActivePanel } from '../../components/active-panel/ActivePanel';
import { EmptyLanding } from './EmptyLanding';
import { useTrackMessages } from '../../hooks/useTracks';
import { useActivePanel, ACTIVE_PANEL_MIN_WIDTH } from '../../hooks/useActivePanel';
import * as exercisesApi from '../../api/exercises';
import type { Exercise, MasteryStatus } from '../../types';
import type { SessionOutletContext } from './SessionLayout';

// The /chat route's content — rendered inside SessionLayout's <Outlet />.
// Shows the active track's conversation, or EmptyLanding if none is active.
export function ChatView() {
  const { activeTrackId, activeTrack, setActiveTrackId, removeTrack, togglePin } =
    useOutletContext<SessionOutletContext>();
  const { messages, isSending, send } = useTrackMessages(activeTrackId);
  const { width: panelWidth, setWidth: setPanelWidth, isOpen: isPanelOpen, toggle: togglePanel } = useActivePanel();
  const resizeHandleRef = useRef<HTMLDivElement>(null);

  const [exercise, setExercise] = useState<Exercise | null>(null);
  const [mastery, setMastery] = useState<MasteryStatus | null>(null);

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

  // Drag-resize: both .conversation and .active-panel have their own
  // min-width in CSS (420px / 320px) — the browser's flex layout already
  // refuses to shrink either past its floor, so this only needs to track
  // the drag delta and let setWidth clamp against the same floor for the
  // persisted value itself (see useActivePanel).
  const handleResizeStart = (event: React.MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = panelWidth;
    resizeHandleRef.current?.classList.add('resizing');
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
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
  };

  if (!activeTrack) {
    return <EmptyLanding />;
  }

  return (
    <>
      <main className="conversation">
        <TrackHeader
          title={activeTrack.title}
          conceptTitle={activeTrack.currentConceptTitle}
          isPinned={activeTrack.pinned}
          onPin={handlePin}
          onDelete={handleDeleteTrack}
          isPanelOpen={isPanelOpen}
          onTogglePanel={togglePanel}
        />
        <MessageList messages={messages} />
        <Composer onSend={send} isSending={isSending} />
      </main>

      {isPanelOpen && (
        <div ref={resizeHandleRef} className="active-panel-resize-handle" onMouseDown={handleResizeStart} />
      )}
      {isPanelOpen && (
        <ActivePanel exercise={exercise} mastery={mastery} onSubmitAnswer={handleSubmitAnswer} width={panelWidth} />
      )}
    </>
  );
}
