import './avatar.css';

interface AvatarProps {
  label: string;
  size?: number;
}

export function Avatar({ label, size = 26 }: AvatarProps) {
  const initial = label.trim().charAt(0).toUpperCase() || '?';
  return (
    <div className="avatar" style={{ width: size, height: size }} aria-hidden="true">
      {initial}
    </div>
  );
}
