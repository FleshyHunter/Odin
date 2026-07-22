import { Composer } from '../../components/conversation/Composer/Composer';

interface EmptyLandingProps {
  onStartTrack: () => void;
}

// Post-login landing state with no active track. The quick-question
// Composer is still a no-op until memoryless chat is wired; Start a track
// opens the shared creation flow supplied by SessionLayout.
export function EmptyLanding({ onStartTrack }: EmptyLandingProps) {
  return (
    <main className="empty-main">
      <h1 className="display">What do you want to learn?</h1>
      <p>Ask a quick question, or start a track to build real progress over time.</p>

      <Composer onSend={() => {}} />

      <button className="start-track-pill" onClick={onStartTrack}>
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
          <line x1="12" y1="5" x2="12" y2="19" />
          <line x1="5" y1="12" x2="19" y2="12" />
        </svg>
        Start a track
      </button>
    </main>
  );
}
