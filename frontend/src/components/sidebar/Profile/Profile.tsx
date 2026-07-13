import { Avatar } from '../../ui/Avatar/Avatar';
import './profile.css';

interface ProfileSwitcherProps {
  displayName: string;
  onClick?: () => void;
}

export function ProfileSwitcher({ displayName, onClick }: ProfileSwitcherProps) {
  return (
    <div className="profile-switcher" role="button" tabIndex={0} onClick={onClick} title={displayName}>
      <Avatar label={displayName} size={34} />
      <span className="profile-name">{displayName}</span>
    </div>
  );
}
