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
  isPanelOpen: boolean;
  onTogglePanel: () => void;
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
      <div className="track-meta">
        {conceptTitle && <span className="concept-pill">{conceptTitle}</span>}
        <button
          className="menu-trigger"
          onClick={onTogglePanel}
          aria-label={isPanelOpen ? 'Hide panel' : 'Show panel'}
          aria-pressed={isPanelOpen}
        >
          {PANEL_TOGGLE_ICON}
        </button>
      </div>
    </header>
  );
}
