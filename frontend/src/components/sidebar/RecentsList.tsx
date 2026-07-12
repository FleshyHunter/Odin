import type { Track } from '../../types';
import { NavItem } from './NavItem/NavItem';

interface RecentsListProps {
  label: string;
  tracks: Track[];
  activeTrackId: string;
  onSelect: (trackId: string) => void;
  emptyMessage: string;
}

// Generic track-row list — renders both the sidebar's "Recents" (all
// tracks) and "Pinned" (tracks.filter(pinned)) sections, since both are
// just a list of tracks with the same NavItem row treatment. Sidebar.tsx
// decides which tracks/label/emptyMessage to pass based on the active section.
export function RecentsList({ label, tracks, activeTrackId, onSelect, emptyMessage }: RecentsListProps) {
  return (
    <>
      <div className="section-label">{label}</div>
      {tracks.length === 0 ? (
        <p className="empty-recents">{emptyMessage}</p>
      ) : (
        tracks.map((track) => (
          <NavItem
            key={track.id}
            baseClassName="recent-item"
            label={track.title}
            active={track.id === activeTrackId}
            onClick={() => onSelect(track.id)}
          />
        ))
      )}
    </>
  );
}
