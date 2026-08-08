import './dragOverlay.css';

interface DragOverlayProps {
  active: boolean;
}

// Full-panel drop-target overlay — rendered by the page (MemorylessLanding.tsx),
// which owns the actual dragenter/dragover/dragleave/drop listeners on its
// .conversation/.empty-main container (position: relative, see Session.css),
// so this covers the whole visible conversation, not just the composer.
export function DragOverlay({ active }: DragOverlayProps) {
  if (!active) return null;

  return (
    <div className="drag-overlay" aria-hidden="true">
      <div className="drag-overlay-box">
        <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.6}>
          <path d="M12 3v13" />
          <path d="M7 9l5-5 5 5" />
          <path d="M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2" />
        </svg>
        <p>Drop files here to add to chat</p>
      </div>
    </div>
  );
}
