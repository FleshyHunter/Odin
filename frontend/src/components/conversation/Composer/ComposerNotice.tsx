import type { ComposerNoticeData } from '../../../types';
import './composerNotice.css';

interface ComposerNoticeProps extends ComposerNoticeData {
  onDismiss: () => void;
}

// Attaches to the top of the composer, sharing its rounded border, rather
// than floating as a separate toast — see Composer.tsx's composer-frame
// wrapper. Modeled directly on Claude Code's own usage-limit banner.
export function ComposerNotice({ tone, message, actionLabel, onAction, onDismiss }: ComposerNoticeProps) {
  return (
    <div className={`composer-notice composer-notice-${tone}`} role="alert">
      <span className="composer-notice-message">{message}</span>
      <div className="composer-notice-actions">
        {actionLabel && onAction && (
          <button type="button" className="composer-notice-action" onClick={onAction}>
            {actionLabel}
          </button>
        )}
        <button type="button" className="composer-notice-dismiss" aria-label="Dismiss" onClick={onDismiss}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
    </div>
  );
}
