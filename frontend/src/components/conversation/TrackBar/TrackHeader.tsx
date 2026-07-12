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
}

export function TrackHeader({ title, conceptTitle, ...menuHandlers }: TrackHeaderProps) {
  return (
    <header className="track-header">
      <TrackMenu title={title} {...menuHandlers} />
      {conceptTitle && (
        <div className="track-meta">
          <span className="concept-pill">{conceptTitle}</span>
        </div>
      )}
    </header>
  );
}
