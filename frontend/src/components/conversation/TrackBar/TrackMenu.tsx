import { useEffect, useRef, useState } from 'react';

interface TrackMenuProps {
  title: string;
  isPinned?: boolean;
  onPin?: () => void;
  onRename?: () => void;
  onChangeProject?: () => void;
  onRemoveFromProject?: () => void;
  onDelete?: () => void;
}

const iconProps = { width: 15, height: 15, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', strokeWidth: 1.6 };

const PIN_ICON = (
  <svg {...iconProps}>
    <path d="M12 2l2.9 6.3L21 9.3l-4.5 4.4 1.1 6.3L12 17l-5.6 3 1.1-6.3L3 9.3l6.1-1z" />
  </svg>
);
const RENAME_ICON = (
  <svg {...iconProps}>
    <path d="M12 20h9" />
    <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
  </svg>
);
const PROJECT_ICON = (
  <svg {...iconProps}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
  </svg>
);
const REMOVE_PROJECT_ICON = (
  <svg {...iconProps}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
    <line x1="9" y1="12" x2="15" y2="12" />
  </svg>
);
const DELETE_ICON = (
  <svg {...iconProps}>
    <polyline points="3 6 5 6 21 6" />
    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
  </svg>
);

// Delete copy states mastery is preserved because mastery is keyed on
// canonical_concept_id, never journey_id (ARCHITECTURE_LOCK.md, Rule 14) —
// deleting a track can never erase already-proven history.
const DELETE_CONFIRM_MESSAGE =
  'Delete this track? Your conversation history for it will be removed. ' +
  'Mastery already earned in it stays recorded.';

export function TrackMenu({
  title,
  isPinned,
  onPin,
  onRename,
  onChangeProject,
  onRemoveFromProject,
  onDelete,
}: TrackMenuProps) {
  const [isOpen, setIsOpen] = useState(false);
  const groupRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    function handleOutsideClick(event: MouseEvent) {
      if (groupRef.current && !groupRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }
    document.addEventListener('click', handleOutsideClick);
    return () => document.removeEventListener('click', handleOutsideClick);
  }, [isOpen]);

  const runAndClose = (action?: () => void) => {
    action?.();
    setIsOpen(false);
  };

  const handleDeleteClick = () => {
    // Ported directly from odin_session.html: a native confirm() dialog,
    // not a custom in-menu confirm step — matching the prototype's actual
    // JS behavior rather than reinterpreting it as a styled modal.
    if (window.confirm(DELETE_CONFIRM_MESSAGE)) {
      onDelete?.();
    }
    setIsOpen(false);
  };

  return (
    // odin_session.html nests the title <h2> and the menu trigger/panel
    // inside the SAME .track-title-group div (not siblings) — title is
    // taken as a prop here rather than split into a separate component,
    // to match that actual DOM structure.
    <div className="track-title-group" ref={groupRef}>
      <h2 className="display">{title}</h2>
      <button
        className="menu-trigger"
        aria-label="Track options"
        aria-haspopup="true"
        aria-expanded={isOpen}
        onClick={(event) => {
          event.stopPropagation();
          setIsOpen((prev) => !prev);
        }}
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>

      <div className={isOpen ? 'track-menu open' : 'track-menu'} onClick={(event) => event.stopPropagation()}>
        <button className="menu-item" onClick={() => runAndClose(onPin)}>
          {PIN_ICON}
          <span>{isPinned ? 'Unpin' : 'Pin'}</span>
        </button>
        <button className="menu-item" onClick={() => runAndClose(onRename)}>
          {RENAME_ICON}
          <span>Rename</span>
        </button>
        <button className="menu-item" onClick={() => runAndClose(onChangeProject)}>
          {PROJECT_ICON}
          <span>Change project</span>
        </button>
        <button className="menu-item" onClick={() => runAndClose(onRemoveFromProject)}>
          {REMOVE_PROJECT_ICON}
          <span>Remove from project</span>
        </button>
        <div className="menu-divider" />
        <button className="menu-item destructive" onClick={handleDeleteClick}>
          {DELETE_ICON}
          <span>Delete track</span>
        </button>
      </div>
    </div>
  );
}
