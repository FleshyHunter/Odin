import { useEffect, useRef } from 'react';
import type { ChatMessage } from '../../../types';
import { MessageBubble } from './MessageBubble';
import './messageList.css';

interface MessageListProps {
  messages: ChatMessage[];
  // Passed straight through to MessageBubble — see its own doc comment.
  onRetry?: (text: string) => void;
}

export function MessageList({ messages, onRetry }: MessageListProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const lastStudentRef = useRef<HTMLDivElement>(null);
  // Reserves real scrollable room below the just-sent exchange (Claude's
  // own behavior, confirmed live: the student's message pins to the top
  // with empty space below it even on a short/first exchange, not only
  // once the conversation is already tall enough to scroll). Height is
  // recalculated on every render of the same exchange (send time AND
  // every later streaming delta), fitted exactly to how much of the
  // viewport the reply hasn't filled yet — NOT just set once to a full
  // viewport-height and left alone, which would leave extra blank
  // scroll room past whatever actually rendered (found live: you could
  // keep scrolling into empty space below a short reply).
  const spacerRef = useRef<HTMLDivElement>(null);
  // Previous render's messages — diffed against the new array to tell a
  // genuine "the student just sent a message" event apart from a fresh
  // thread load/switch (getThread's hydration replaces the whole array
  // in one shot; useMemorylessChat's send() appends a new student+tutor
  // pair to the SAME array).
  const prevMessagesRef = useRef<ChatMessage[]>([]);

  useEffect(() => {
    const prevMessages = prevMessagesRef.current;
    prevMessagesRef.current = messages;

    // Different conversation entirely (first message's id changed, or
    // there was nothing before) — a fresh load/thread switch, not a
    // live send. Claude/ChatGPT both jump straight to the bottom for
    // this case (show where the conversation left off), not a "pin to
    // top with reserved space" — matches this app's own prior behavior.
    const isFreshLoad = prevMessages.length === 0 || messages[0]?.id !== prevMessages[0]?.id;
    if (isFreshLoad) {
      // No reserved space belongs here — it was fitted for a different
      // exchange, if any existed at all.
      if (spacerRef.current) spacerRef.current.style.minHeight = '0px';
      endRef.current?.scrollIntoView({ block: 'end' });
      return;
    }

    // The message newly appended right where the old array ended.
    // useMemorylessChat's send() appends student THEN the empty tutor
    // placeholder in the same update, so this is the student message
    // exactly when a real send just happened.
    const newlyAppended = messages.length > prevMessages.length ? messages[prevMessages.length] : null;
    if (newlyAppended?.role === 'student' && lastStudentRef.current) {
      // Instant, not smooth — both references snap immediately here.
      lastStudentRef.current.scrollIntoView({ block: 'start', behavior: 'instant' });
    }

    // Runs both right after the scroll above (send time) AND on every
    // later streaming delta to the SAME exchange (deltas re-render with
    // a new `messages` array reference even though length is
    // unchanged, so this effect re-fires on each one) — fits the spacer
    // to EXACTLY how much of one viewport-height the reply hasn't
    // filled yet, measured from the student message's actual current
    // top position. Shrinks as the reply grows; clamped to 0 once the
    // reply alone already exceeds a full viewport, so a long reply
    // scrolls normally instead of leaving (or needing) any reservation.
    if (lastStudentRef.current && containerRef.current && spacerRef.current) {
      const messageTop = lastStudentRef.current.getBoundingClientRect().top;
      const contentBottom = spacerRef.current.getBoundingClientRect().top;
      const available = containerRef.current.clientHeight;
      spacerRef.current.style.minHeight = `${Math.max(0, available - (contentBottom - messageTop))}px`;
    }
  }, [messages]);

  // Only the LAST student message needs a ref (to scroll it into place
  // right after sending) — every other message renders exactly as
  // before, no extra wrapper.
  let lastStudentMessageId: string | null = null;
  for (let i = messages.length - 1; i >= 0; i--) {
    if (messages[i].role === 'student') {
      lastStudentMessageId = messages[i].id;
      break;
    }
  }

  return (
    <div className="messages" ref={containerRef}>
      <div className="messages-inner">
        {messages.length === 0 && <p className="panel-footnote">Say something to get started.</p>}
        {messages.map((message) =>
          message.id === lastStudentMessageId ? (
            <div key={message.id} ref={lastStudentRef}>
              <MessageBubble message={message} onRetry={onRetry} />
            </div>
          ) : (
            <MessageBubble key={message.id} message={message} onRetry={onRetry} />
          ),
        )}
        <div ref={endRef} />
        <div ref={spacerRef} className="messages-spacer" />
      </div>
    </div>
  );
}
