import { TrackMenu } from './TrackMenu';

interface TrackHeaderProps {
  title: string;
  conceptTitle: string | null;
  isPinned?: boolean;
  onPin?: () => void;
  onRename?: () => void;
  onChangeProject?: () => void;
  onRemoveFromProject?: () => void;
  onDelete?: () => void;
  // Memoryless mode's placeholder header (deferred.md #74) has no
  // ActivePanel content (no exercises/mastery for a chat with no
  // journey yet) — so unlike a real track, there's nothing for this
  // side to toggle. Optional rather than required lets that call site
  // omit the whole panel-toggle button instead of wiring a fake one.
  isPanelOpen?: boolean;
  onTogglePanel?: () => void;
  // Passed straight through to TrackMenu via ...menuHandlers below —
  // memoryless mode's one real action.
  onAddToTrack?: () => void;
}

// Rounded square divided nearer its right edge — mirrors the sidebar
// toggle's icon (divided nearer the left) to read as "toggle the panel on
// THIS side" for each control.
const PANEL_TOGGLE_ICON = (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7}>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <line x1="15" y1="4" x2="15" y2="20" />
  </svg>
);

export function TrackHeader({
  title,
  conceptTitle,
  isPanelOpen,
  onTogglePanel,
  ...menuHandlers
}: TrackHeaderProps) {
  return (
    <header className="track-header">
      <TrackMenu title={title} {...menuHandlers} />
      {(conceptTitle || onTogglePanel) && (
        <div className="track-meta">
          {conceptTitle && <span className="concept-pill">{conceptTitle}</span>}
          {onTogglePanel && (
            <button
              className="menu-trigger panel-toggle"
              onClick={onTogglePanel}
              aria-label={isPanelOpen ? 'Hide panel' : 'Show panel'}
              aria-pressed={isPanelOpen}
            >
              {PANEL_TOGGLE_ICON}
            </button>
          )}
        </div>
      )}
    </header>
  );
}
