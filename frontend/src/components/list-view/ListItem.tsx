import './listItem.css';

interface ListItemProps {
  title: string;
  date: string;
  description?: string | null;
  onClick?: () => void;
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

// Shared row for any browse-all list (Projects, Tracks) — takes plain
// primitives rather than a Project/Track directly, since the two types
// don't share a shape (Track has no description and uses lastActiveAt
// instead of updatedAt). Each page maps its own data onto these props.
export function ListItem({ title, date, description, onClick }: ListItemProps) {
  return (
    <div className="list-item" role="button" tabIndex={0} onClick={onClick}>
      <div className="list-item-row">
        <span className="list-item-title">{title}</span>
        <span className="list-item-date">{formatDate(date)}</span>
      </div>
      {description && <p className="list-item-description">{description}</p>}
    </div>
  );
}
