import type { Track } from '../../../types';
import { Wordmark } from '../../ui/WordMark/Wordmark';
import { NavItem } from '../NavItem/NavItem';
import { RecentsList } from '../RecentsList';
import { ProfileSwitcher } from '../Profile/Profile';
import { useSidebarCollapsed } from '../../../hooks/useSidebarCollapsed';
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

const TOGGLE_ICON = (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7}>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <line x1="9" y1="4" x2="9" y2="20" />
  </svg>
);

// "Compose" icon (ChatGPT's new-chat glyph) — a square outline with a
// pencil crossing its top-right corner. Chosen deliberately over a plain
// "+": as a filled/outlined square it already carries enough visual
// weight as a plain NavItem row, unlike a thin cross which would need an
// extra badge/background to read as an action (see New track's history).
const COMPOSE_ICON = (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7} strokeLinecap="round" strokeLinejoin="round">
    <path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
    <path d="M18.375 2.625a1 1 0 0 1 3 3l-9.013 9.014a2 2 0 0 1-.853.505l-2.873.84a.5.5 0 0 1-.62-.62l.84-2.873a2 2 0 0 1 .506-.852z" />
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
  const { collapsed, toggle } = useSidebarCollapsed();

  return (
    <aside className={collapsed ? 'sidebar sidebar-collapsed' : 'sidebar'}>
      <div className="sidebar-top-row">
        <Wordmark showIcon={false} className="sidebar-wordmark" />
        <button
          className="sidebar-toggle"
          onClick={toggle}
          aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
          aria-pressed={collapsed}
        >
          {TOGGLE_ICON}
        </button>
      </div>

      <NavItem icon={COMPOSE_ICON} label="New track" onClick={onNewTrack} />

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

      {!collapsed &&
        (activeSection === 'pinned' ? (
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
        ))}

      <div className="sidebar-spacer" />

      <ProfileSwitcher displayName={profileName} />
    </aside>
  );
}
