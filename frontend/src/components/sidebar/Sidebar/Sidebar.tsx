import type { Track } from '../../../types';
import { Wordmark } from '../../ui/WordMark/Wordmark';
import { NewTrackButton } from '../NewTrackButton/NewTrackButton';
import { NavItem } from '../NavItem/NavItem';
import { RecentsList } from '../RecentsList';
import { ProfileSwitcher } from '../Profile/Profile';
import './sidebar.css';

export type SidebarSection = 'tracks' | 'projects' | 'pinned';

interface SidebarProps {
  tracks: Track[];
  activeTrackId: string;
  onSelectTrack: (trackId: string) => void;
  onNewTrack?: () => void;
  profileName: string;
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

const TRACKS_ICON = (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7}>
    <line x1="4" y1="6" x2="20" y2="6" />
    <line x1="4" y1="12" x2="20" y2="12" />
    <line x1="4" y1="18" x2="14" y2="18" />
  </svg>
);

const PROJECTS_ICON = (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
  </svg>
);

const PINNED_ICON = (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7}>
    <path d="M12 2l2.9 6.3L21 9.3l-4.5 4.4 1.1 6.3L12 17l-5.6 3 1.1-6.3L3 9.3l6.1-1z" />
  </svg>
);

export function Sidebar({
  tracks,
  activeTrackId,
  onSelectTrack,
  onNewTrack,
  profileName,
  activeSection,
  onSectionChange,
}: SidebarProps) {
  return (
    <aside className="sidebar">
      <Wordmark showIcon={false} className="sidebar-wordmark" />

      <NewTrackButton onClick={onNewTrack} />

      <NavItem
        icon={TRACKS_ICON}
        label="Tracks"
        active={activeSection === 'tracks'}
        onClick={() => onSectionChange('tracks')}
      />
      <NavItem
        icon={PROJECTS_ICON}
        label="Projects"
        active={activeSection === 'projects'}
        onClick={() => onSectionChange('projects')}
      />
      <NavItem
        icon={PINNED_ICON}
        label="Pinned"
        active={activeSection === 'pinned'}
        onClick={() => onSectionChange('pinned')}
      />

      {activeSection === 'pinned' ? (
        <RecentsList
          label="Pinned"
          tracks={tracks.filter((track) => track.pinned)}
          activeTrackId={activeTrackId}
          onSelect={onSelectTrack}
          emptyMessage="Nothing pinned yet — pin a track from its menu to see it here."
        />
      ) : (
        <RecentsList
          label="Recents"
          tracks={tracks}
          activeTrackId={activeTrackId}
          onSelect={onSelectTrack}
          emptyMessage="Nothing yet — start a track and it'll show up here."
        />
      )}

      <div className="sidebar-spacer" />

      <ProfileSwitcher displayName={profileName} />
    </aside>
  );
}
