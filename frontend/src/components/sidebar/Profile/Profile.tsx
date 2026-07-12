import { Avatar } from '../../ui/Avatar/Avatar';
import './profile.css';

interface ProfileSwitcherProps {
  displayName: string;
  onClick?: () => void;
}

export function ProfileSwitcher({ displayName, onClick }: ProfileSwitcherProps) {
  return (
    <div className="profile-switcher" role="button" tabIndex={0} onClick={onClick}>
      <Avatar label={displayName} />
      <span>{displayName}</span>
    </div>
  );
}
