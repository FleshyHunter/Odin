import './wordmark.css';

interface WordmarkProps {
  className?: string;
  /** odin_session.html's sidebar wordmark is text-only (no icon) —
   *  set false to match that markup exactly. Default true matches
   *  odin_login.html's nav wordmark (icon + text). */
  showIcon?: boolean;
}

// Raven glyph ported from odin_login.html / odin_session.html. The real
// logo is explicitly TBD/held (ARCHITECTURE_LOCK.md, Repository Structure
// note on Wordmark.tsx) — this is a placeholder, not final branding.
export function Wordmark({ className, showIcon = true }: WordmarkProps) {
  if (!showIcon) {
    const classes = ['wordmark', 'display', className].filter(Boolean).join(' ');
    return <div className={classes}>Odin</div>;
  }

  return (
    <div className={className ? `wordmark ${className}` : 'wordmark'}>
      <svg width="17" height="17" viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <path d="M16.5 14 L22.5 9.5 L19.5 17.5 Z" fill="#E8B76A" />
        <ellipse cx="12" cy="14" rx="6" ry="4.6" fill="#E8B76A" transform="rotate(-12 12 14)" />
        <circle cx="6.8" cy="8.6" r="3.2" fill="#E8B76A" />
        <path d="M4 8.7 L7.6 6.8 L7.6 10.6 Z" fill="#E8B76A" />
      </svg>
      <span className="display">Odin</span>
    </div>
  );
}
