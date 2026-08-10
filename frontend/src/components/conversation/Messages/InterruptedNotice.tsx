import './interruptedNotice.css';

interface InterruptedNoticeProps {
  onRetry?: () => void;
}

// Rendered inline in the message thread itself (by MessageBubble), right
// where a cancelled turn's reply would have continued — deliberately NOT
// the composer-level ComposerNotice mechanism (that's a dismissible
// banner tied to the composer's current state, not a durable artifact of
// one specific past turn). Neutral tone on purpose: unlike
// ComposerNotice's warning/error tones, the student stopping their own
// turn isn't a failure.
export function InterruptedNotice({ onRetry }: InterruptedNoticeProps) {
  return (
    <div className="interrupted-notice">
      <svg
        className="interrupted-notice-icon"
        width="15"
        height="15"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        aria-hidden="true"
      >
        <circle cx="12" cy="12" r="10" />
        <line x1="12" y1="11" x2="12" y2="16" />
        <line x1="12" y1="8" x2="12" y2="8" />
      </svg>
      <span className="interrupted-notice-message">Odin's response was interrupted.</span>
      {onRetry && (
        <button type="button" className="interrupted-notice-retry" onClick={onRetry}>
          Try again
        </button>
      )}
    </div>
  );
}
