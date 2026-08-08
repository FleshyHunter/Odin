import { useRef, useState, type DragEvent } from 'react';
import { Composer } from '../../components/conversation/Composer/Composer';
import { DragOverlay } from '../../components/conversation/Composer/DragOverlay/DragOverlay';
import { MessageList } from '../../components/conversation/Messages/MessageList';
import { useMemorylessChat } from '../../hooks/useMemorylessChat';

interface MemorylessLandingProps {
  onStartTrack: () => void;
  // deferred.md #43: a /chat/:id URL that isn't a known mock track is
  // treated as a real memoryless thread id — this seeds the hook so it
  // fetches that thread's real history instead of starting blank.
  threadId?: string | null;
  // Fires exactly once, only when a BRAND NEW thread (started from a
  // bare /chat, threadId prop null) gets its first real id back from
  // the backend — lets the caller push /chat/:id via router history.
  onThreadCreated?: (threadId: string) => void;
}

// The /chat route's no-active-track state (deferred.md #51). Renders the
// centered "what do you want to learn?" prompt until the first message
// is sent, then becomes a real streamed conversation against
// POST /memoryless/messages (no track, no journey — same bare-turn scope
// as the rest of Block 11/12; TrackHeader/ActivePanel are track-specific
// and don't apply here).
export function MemorylessLanding({ onStartTrack, threadId = null, onThreadCreated }: MemorylessLandingProps) {
  const {
    messages,
    isSending,
    isHydrating,
    send,
    cancel,
    composerNotice,
    dismissComposerNotice,
    pendingFiles,
    attachments,
    requestAttach,
    confirmAttachRole,
    cancelPendingFile,
    retryAttachment,
    removeAttachment,
  } = useMemorylessChat(threadId, onThreadCreated);

  // deferred.md #37: dragenter/dragleave fire on every child element the
  // cursor crosses while dragging over the container, not just on
  // enter/exit of the container itself — a plain boolean would flicker
  // the overlay on/off as the cursor moves over MessageList/Composer/etc.
  // A depth counter (matches the standard DnD-overlay pattern) only drops
  // to "not dragging" once the count actually returns to zero.
  const [isDraggingOver, setIsDraggingOver] = useState(false);
  const dragCounterRef = useRef(0);

  const handleDragEnter = (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    if (!event.dataTransfer.types.includes('Files')) return;
    dragCounterRef.current += 1;
    setIsDraggingOver(true);
  };
  const handleDragOver = (event: DragEvent<HTMLElement>) => {
    // Required for onDrop to ever fire at all — a bare dragover with no
    // preventDefault tells the browser "not a valid drop target."
    event.preventDefault();
  };
  const handleDragLeave = (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    dragCounterRef.current = Math.max(0, dragCounterRef.current - 1);
    if (dragCounterRef.current === 0) setIsDraggingOver(false);
  };
  const handleDrop = (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    dragCounterRef.current = 0;
    setIsDraggingOver(false);
    if (event.dataTransfer.files.length > 0) {
      requestAttach(Array.from(event.dataTransfer.files));
    }
  };

  const dragHandlers = {
    onDragEnter: handleDragEnter,
    onDragOver: handleDragOver,
    onDragLeave: handleDragLeave,
    onDrop: handleDrop,
  };
  const attachProps = {
    attachments,
    pendingFiles,
    onAttachFiles: requestAttach,
    onConfirmAttachRole: confirmAttachRole,
    onCancelPendingFile: cancelPendingFile,
    onRemoveAttachment: removeAttachment,
    onRetryAttachment: retryAttachment,
  };

  if (isHydrating) {
    return <main className="conversation" />;
  }

  if (messages.length === 0) {
    return (
      <main className="empty-main" {...dragHandlers}>
        <DragOverlay active={isDraggingOver} />
        <h1 className="display">What do you want to learn?</h1>
        <p>Ask a quick question, or start a track to build real progress over time.</p>

        <Composer
          onSend={send}
          isSending={isSending}
          onStop={cancel}
          notice={composerNotice}
          onDismissNotice={dismissComposerNotice}
          {...attachProps}
        />

        <button className="start-track-pill" onClick={onStartTrack}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
            <line x1="12" y1="5" x2="12" y2="19" />
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
          Start a track
        </button>
      </main>
    );
  }

  return (
    <main className="conversation" {...dragHandlers}>
      <DragOverlay active={isDraggingOver} />
      <MessageList messages={messages} />
      <Composer
        onSend={send}
        isSending={isSending}
        onStop={cancel}
        notice={composerNotice}
        onDismissNotice={dismissComposerNotice}
        {...attachProps}
      />
    </main>
  );
}
