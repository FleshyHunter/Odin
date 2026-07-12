import { Fragment, type ReactNode } from 'react';
import type { ChatMessage } from '../../../types';
import './messageBubble.css';

interface MessageBubbleProps {
  message: ChatMessage;
}

const TUTOR_AVATAR = (
  <div className="bubble-avatar">
    <svg width="13" height="13" viewBox="0 0 24 24" fill="#E8B76A" aria-hidden="true">
      <path d="M16.5 14 L22.5 9.5 L19.5 17.5 Z" />
      <ellipse cx="12" cy="14" rx="6" ry="4.6" transform="rotate(-12 12 14)" />
      <circle cx="6.8" cy="8.6" r="3.2" />
      <path d="M4 8.7 L7.6 6.8 L7.6 10.6 Z" />
    </svg>
  </div>
);

// Tutor message text may contain a lightweight `{{term}}` marker for the
// glow-highlighted vocabulary term seen in odin_session.html
// (<em class="term">eigenvalues</em>). Parsed safely here — split on a
// known marker, never dangerouslySetInnerHTML — since message text will
// eventually come from the AI service and should not be trusted as raw HTML.
function renderInlineTerms(line: string, keyPrefix: string): ReactNode[] {
  const parts = line.split(/(\{\{[^}]+\}\})/g);
  return parts.map((part, index) => {
    const match = part.match(/^\{\{([^}]+)\}\}$/);
    if (match) {
      return (
        <em key={`${keyPrefix}-${index}`} className="term">
          {match[1]}
        </em>
      );
    }
    return <Fragment key={`${keyPrefix}-${index}`}>{part}</Fragment>;
  });
}

function renderMessageText(text: string): ReactNode {
  return text.split('\n').map((line, index) => (
    <Fragment key={index}>
      {index > 0 && <br />}
      {renderInlineTerms(line, `line-${index}`)}
    </Fragment>
  ));
}

export function MessageBubble({ message }: MessageBubbleProps) {
  if (message.role === 'student') {
    return (
      <div className="msg student">
        <div className="bubble">{renderMessageText(message.text)}</div>
      </div>
    );
  }

  return (
    <div className="msg tutor">
      {TUTOR_AVATAR}
      <div className="bubble">{renderMessageText(message.text)}</div>
    </div>
  );
}
