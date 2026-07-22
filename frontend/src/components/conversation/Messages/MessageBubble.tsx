import { Fragment, type ReactNode } from 'react';
import type { ChatMessage } from '../../../types';
import './messageBubble.css';

interface MessageBubbleProps {
  message: ChatMessage;
}

const TUTOR_AVATAR = (
  <div className="bubble-avatar">
    <img src="/assets/odin-icon-large.png" alt="" aria-hidden="true" />
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
