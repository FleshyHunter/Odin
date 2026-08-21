import { useState, type MouseEvent } from 'react';
import { Tooltip } from '../../ui/Tooltip/Tooltip';
import './messageUtil.css';

interface MessageUtilProps {
  side: 'user' | 'ai';
  text: string;
  timestamp: string;
}

const SHORT_MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

function pad2(n: number): string {
  return n < 10 ? `0${n}` : `${n}`;
}

function formatTime(date: Date): string {
  return `${pad2(date.getHours())}:${pad2(date.getMinutes())}`;
}

function formatShortDate(date: Date, includeYear: boolean): string {
  const base = `${date.getDate()} ${SHORT_MONTHS[date.getMonth()]}`;
  return includeYear ? `${base} ${date.getFullYear()}` : base;
}

// Inline label: time-only if sent today, otherwise a short date (year
// only appended once it's not the current year — no point cluttering
// the common case with a year that's already obvious from context).
function formatShortTimestamp(iso: string): string {
  const date = new Date(iso);
  const now = new Date();
  const isToday =
    date.getFullYear() === now.getFullYear() && date.getMonth() === now.getMonth() && date.getDate() === now.getDate();
  if (isToday) return formatTime(date);
  return formatShortDate(date, date.getFullYear() !== now.getFullYear());
}

// Tooltip label: always the full absolute stamp, e.g. "11 Aug 2026, 16:57".
function formatFullTimestamp(iso: string): string {
  const date = new Date(iso);
  return `${formatShortDate(date, true)}, ${formatTime(date)}`;
}

function CopyIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8}>
      <rect x="8" y="8" width="13" height="13" rx="2" />
      <path d="M4 16V5a1 1 0 0 1 1-1h11" />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}

// Copy + timestamp, shown only once the row above it (MessageBubble's
// outer container) is hovered — see messageBubble.css for that toggle.
// Same two children, same DOM order, for both sides — `side` only flips
// which edge the row hugs and which end "copy" lands on via CSS
// (row-reverse for user), not a structural difference.
export function MessageUtil({ side, text, timestamp }: MessageUtilProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async (event: MouseEvent<HTMLButtonElement>) => {
    // Found live: a clicked button keeps browser focus after the click,
    // and :focus-within (messageUtil.css, for real keyboard-tab access)
    // kept this whole row visible even once the mouse moved away right
    // after — outside the "only pop up on hover" spec. Blurring here
    // only affects the CLICK path; tabbing to the button with a
    // keyboard still reveals/keeps it via that same :focus-within rule.
    event.currentTarget.blur();
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard access denied/unavailable — no destructive fallback
      // worth adding for a "nice to have" action; the button just does
      // nothing visible, same as it would if the user cancelled a
      // permission prompt.
    }
  };

  return (
    <div className={`msg-util msg-util-${side}`}>
      <Tooltip label={copied ? 'Copied' : 'Copy'}>
        <button type="button" className="msg-util-btn" aria-label="Copy" onClick={handleCopy}>
          {copied ? <CheckIcon /> : <CopyIcon />}
        </button>
      </Tooltip>
      <Tooltip label={formatFullTimestamp(timestamp)}>
        <span className="msg-util-timestamp" tabIndex={0}>
          {formatShortTimestamp(timestamp)}
        </span>
      </Tooltip>
    </div>
  );
}
