import { useEffect, useState } from 'react';
import { useOutletContext } from 'react-router-dom';
import { TrackHeader } from '../../components/conversation/TrackBar/TrackHeader';
import { MessageList } from '../../components/conversation/Messages/MessageList';
import { Composer } from '../../components/conversation/Composer/Composer';
import { ActivePanel } from '../../components/active-panel/ActivePanel';
import { EmptyLanding } from './EmptyLanding';
import { useTrackMessages } from '../../hooks/useTracks';
import * as exercisesApi from '../../api/exercises';
import type { Exercise, MasteryStatus } from '../../types';
import type { SessionOutletContext } from './SessionLayout';

// The /chat route's content — rendered inside SessionLayout's <Outlet />.
// Shows the active track's conversation, or EmptyLanding if none is active.
export function ChatView() {
  const { activeTrackId, activeTrack, setActiveTrackId, removeTrack, togglePin } =
    useOutletContext<SessionOutletContext>();
  const { messages, isSending, send } = useTrackMessages(activeTrackId);

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
        />
        <MessageList messages={messages} />
        <Composer onSend={send} isSending={isSending} />
      </main>

      <ActivePanel exercise={exercise} mastery={mastery} onSubmitAnswer={handleSubmitAnswer} />
    </>
  );
}
