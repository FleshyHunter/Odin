import { useEffect, useRef, useState, type DragEvent } from 'react';
import { Composer } from '../../components/conversation/Composer/Composer';
import { DragOverlay } from '../../components/conversation/Composer/DragOverlay/DragOverlay';
import { MessageList } from '../../components/conversation/Messages/MessageList';
import { TrackHeader } from '../../components/conversation/TrackBar/TrackHeader';
import { useMemorylessChat } from '../../hooks/useMemorylessChat';
import './memorylessLanding.css';

interface MemorylessLandingProps {
  // deferred.md #74: accepts an optional thread_id now — the "Start a
  // track" pill below still calls this with zero args (start something
  // new, nothing to attach), but TrackHeader's "Add to a track" passes
  // the CURRENT thread_id so TrackModal seeds a real conversion
  // (openCreateTrackModal(initialThreadId?) → TrackModal.initialThreadId
  // → convertMemorylessThread) instead of just opening a blank modal.
  onStartTrack: (threadId?: string) => void;
  // deferred.md #43: a /chat/:id URL that isn't a known mock track is
  // treated as a real memoryless thread id — this seeds the hook so it
  // fetches that thread's real history instead of starting blank.
  threadId?: string | null;
  // Fires exactly once, only when a BRAND NEW thread (started from a
  // bare /chat, threadId prop null) gets its first real id back from
  // the backend — lets the caller push /chat/:id via router history.
  onThreadCreated?: (threadId: string) => void;
  // deferred.md #8: the student accepted the "start a study thread?"
  // nudge — the caller opens TrackModal seeded with this thread_id so
  // it can call POST .../convert (deferred.md #17) once a real journey
  // exists.
  onConvert?: (threadId: string) => void;
}

// The /chat route's no-active-track state (deferred.md #51). Renders the
// centered "what do you want to learn?" prompt until the first message
// is sent, then becomes a real streamed conversation against
// POST /memoryless/messages (no track, no journey — same bare-turn scope
// as the rest of Block 11/12; TrackHeader/ActivePanel are track-specific
// and don't apply here).
export function MemorylessLanding({ onStartTrack, threadId = null, onThreadCreated, onConvert }: MemorylessLandingProps) {
  const {
    messages,
    isSending,
    isHydrating,
    send,
    cancel,
    composerNotice,
    dismissComposerNotice,
    attachments,
    requestAttach,
    removeAttachment,
  } = useMemorylessChat(threadId, onThreadCreated, onConvert);

  // deferred.md #37: dragenter/dragleave fire on every child element the
  // cursor crosses while dragging over the container, not just on
  // enter/exit of the container itself — a plain boolean would flicker
  // the overlay on/off as the cursor moves over MessageList/Composer/etc.
  // A depth counter (matches the standard DnD-overlay pattern) only drops
  // to "not dragging" once the count actually returns to zero.
  const [isDraggingOver, setIsDraggingOver] = useState(false);
  const dragCounterRef = useRef(0);

  // deferred.md #99/#100 — the empty state starts genuinely centered
  // (.empty-main's own justify-content: center, exact by construction,
  // no guessed offset). The moment the composer grows past its resting
  // single-line height, this measures where the title is ACTUALLY
  // sitting right now (the real centered position, not an
  // approximation) and locks .empty-main to that exact offset via an
  // inline style, switching off dynamic centering so further growth
  // only pushes the "Start a track" pill below further down — the
  // title never moves once locked. Reverts to null (dynamic centering
  // resumes) if the composer shrinks back down to empty, so deleting
  // everything genuinely re-centers rather than staying pinned at a
  // stale offset.
  const emptyMainRef = useRef<HTMLElement>(null);
  const titleRef = useRef<HTMLHeadingElement>(null);
  const composerWrapperRef = useRef<HTMLDivElement>(null);
  const restHeightRef = useRef<number | null>(null);
  const [lockedPaddingTop, setLockedPaddingTop] = useState<number | null>(null);

  useEffect(() => {
    const wrapper = composerWrapperRef.current;
    if (!wrapper) return;

    const observer = new ResizeObserver(([entry]) => {
      const height = entry.contentRect.height;
      // First observation after mount establishes the resting (single-
      // line) height — nothing to compare against yet.
      if (restHeightRef.current === null) {
        restHeightRef.current = height;
        return;
      }
      if (height > restHeightRef.current) {
        // Only measure/lock once — later ticks while already grown
        // shouldn't re-measure (the title's real position no longer
        // reflects "centered," it reflects "already locked").
        if (lockedPaddingTop === null && emptyMainRef.current && titleRef.current) {
          const containerTop = emptyMainRef.current.getBoundingClientRect().top;
          const titleTop = titleRef.current.getBoundingClientRect().top;
          setLockedPaddingTop(titleTop - containerTop);
        }
      } else if (height <= restHeightRef.current) {
        setLockedPaddingTop(null);
      }
    });
    observer.observe(wrapper);
    return () => observer.disconnect();
  }, [lockedPaddingTop]);

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
  // deferred.md #92: no pendingFiles/onConfirmAttachRole/onCancelPendingFile
  // — the role-picker modal is gone for memoryless mode. Composer.tsx
  // itself needs no changes for this: UploadRoleModal already renders
  // nothing when there's no pending file (its own "file: null means
  // nothing pending" contract), so simply never producing one here is
  // enough. onRetryAttachment points at the same removeAttachment as
  // onRemoveAttachment — there's no clean per-file retry anymore once a
  // whole turn (text + every attachment) has already gone out together;
  // "retry" here just means "take it off and try again."
  const attachProps = {
    attachments,
    onAttachFiles: requestAttach,
    onRemoveAttachment: removeAttachment,
    onRetryAttachment: removeAttachment,
    allowEmptyTextWithAttachments: true,
  };

  if (isHydrating) {
    return <main className="conversation" />;
  }

  if (messages.length === 0) {
    return (
      <main
        className="empty-main"
        ref={emptyMainRef}
        style={lockedPaddingTop !== null ? { justifyContent: 'flex-start', paddingTop: lockedPaddingTop } : undefined}
        {...dragHandlers}
      >
        <DragOverlay active={isDraggingOver} />
        <h1 className="display" ref={titleRef}>
          What do you want to learn?
        </h1>
        <p>Ask a quick question, or start a track to build real progress over time.</p>

        <div ref={composerWrapperRef} style={{ width: '100%' }}>
          <Composer
            onSend={send}
            isSending={isSending}
            onStop={cancel}
            notice={composerNotice}
            onDismissNotice={dismissComposerNotice}
            {...attachProps}
          />
        </div>

        <button className="start-track-pill" onClick={() => onStartTrack()}>
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
      {/* Gated on the real thread_id (the threadId PROP, which only
          becomes non-null once the URL has updated to /chat/:id — see
          onThreadCreated above), not on messages.length: the student's
          own message is added to `messages` optimistically before the
          backend has assigned anything, so gating on message count
          would make "Add to a track" clickable for a moment before
          there's a real thread_id for it to act on. */}
      {threadId !== null && (
        <TrackHeader
          title="New chat"
          conceptTitle={null}
          // deferred.md #74 — now passes the real, live thread_id (the
          // conditional above already narrows it to `string`), so
          // TrackModal seeds a real conversion via convertMemorylessThread
          // once the new journey exists, instead of just opening a blank
          // "New track" flow with nothing to attach.
          onAddToTrack={() => onStartTrack(threadId)}
        />
      )}
      <MessageList messages={messages} onRetry={send} />
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
