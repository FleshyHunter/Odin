import { useEffect, useRef } from 'react';
import type { ChatMessage } from '../../../types';
import { MessageBubble } from './MessageBubble';
import './messageList.css';

interface MessageListProps {
  messages: ChatMessage[];
}

export function MessageList({ messages }: MessageListProps) {
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: 'end' });
  }, [messages.length]);

  return (
    <div className="messages">
      {messages.length === 0 && <p className="panel-footnote">Say something to get started.</p>}
      {messages.map((message) => (
        <MessageBubble key={message.id} message={message} />
      ))}
      <div ref={endRef} />
    </div>
  );
}
