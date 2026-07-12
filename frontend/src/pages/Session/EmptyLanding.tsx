import { Composer } from '../../components/conversation/Composer/Composer';

// Post-login landing state when the user has zero tracks. Reuses the same
// Composer as an active track's conversation (was a hand-rolled .ask-bar
// before) — tidy-up only, no real behavior wired yet: onSend is a no-op,
// same "no functionality" status as the "Start a track" pill below.
export function EmptyLanding() {
  return (
    <main className="empty-main">
      <h1 className="display">What do you want to learn?</h1>
      <p>Ask a quick question, or start a track to build real progress over time.</p>

      <Composer onSend={() => {}} />

      <button className="start-track-pill">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
          <line x1="12" y1="5" x2="12" y2="19" />
          <line x1="5" y1="12" x2="19" y2="12" />
        </svg>
        Start a track
      </button>
    </main>
  );
}
