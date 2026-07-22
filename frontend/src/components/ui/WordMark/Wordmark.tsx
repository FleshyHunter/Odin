import './wordmark.css';

interface WordmarkProps {
  className?: string;
  /** odin_session.html's sidebar wordmark is text-only (no icon) —
   *  set false to match that markup exactly. Default true matches
   *  odin_login.html's nav wordmark (icon + text). */
  showIcon?: boolean;
  onClick?: () => void;
}

export function Wordmark({ className, showIcon = true, onClick }: WordmarkProps) {
  const classes = ['wordmark', !showIcon && 'display', onClick && 'wordmark-button', className]
    .filter(Boolean)
    .join(' ');

  const content = showIcon ? (
    <>
      <img
        className="wordmark-icon"
        src="/assets/odin-icon-large.png"
        alt=""
        aria-hidden="true"
      />
      <span className="display">Odin</span>
    </>
  ) : (
    'Odin'
  );

  if (onClick) {
    return (
      <button type="button" className={classes} onClick={onClick} aria-label="Go to chat home">
        {content}
      </button>
    );
  }

  return <div className={classes}>{content}</div>;
}
