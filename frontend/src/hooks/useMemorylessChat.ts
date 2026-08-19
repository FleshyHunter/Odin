import { useCallback, useEffect, useRef, useState } from 'react';
import * as memorylessApi from '../api/memoryless';
import * as uploadsApi from '../api/uploads';
import type { Attachment, ChatMessage, ComposerNoticeData, PendingFile, UploadRole } from '../types';

// deferred.md #8: PRD.md's own "3-5" range has no exact number — the
// middle, picked once as a fixed UX value (same "not an operational
// knob" reasoning turn.rs already uses for its own HISTORY_WINDOW_MESSAGES,
// not a Rule 12 env var). One exchange == one student + one tutor
// message, so this compares against Math.floor(messages.length / 2).
const NUDGE_AFTER_EXCHANGES = 4;

// Real memoryless chat (deferred.md #51) — the /chat landing composer's
// previous onSend={() => {}} no-op replaced with the actual backend
// turn (POST /memoryless/messages, streamed). thread_id lives in a ref,
// not state: it's an internal bookkeeping detail (which Redis-staged
// thread to keep appending to) that no render needs to react to.
//
// initialThreadId (deferred.md #43): when a /chat/:id URL names a real,
// still-live memoryless thread, its message history is fetched via
// GET /memoryless/threads/:id and used to seed the conversation instead
// of starting blank — a direct visit/reload now actually restores what
// was there, not just a URL with nothing behind it. onThreadId fires
// once, only the first time THIS hook learns a real thread_id (i.e. a
// brand new thread created from a bare /chat) — never on every
// message of an already-known thread — so a caller can push the URL to
// /chat/:id via router history exactly once, matching DESIGN.md's
// "URL updates via router history API only... never a full page
// reload" spec.
//
// onNudgeAccept (deferred.md #8): fires when the student accepts the
// "start a study thread?" nudge below — the caller is responsible for
// opening TrackModal with this thread_id so it can call
// POST .../convert (deferred.md #17) once a real journey exists.
export function useMemorylessChat(
  initialThreadId: string | null = null,
  onThreadId?: (threadId: string) => void,
  onNudgeAccept?: (threadId: string) => void,
) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isSending, setIsSending] = useState(false);
  const [isHydrating, setIsHydrating] = useState(initialThreadId !== null);
  const [composerNotice, setComposerNotice] = useState<ComposerNoticeData | null>(null);
  const [pendingFiles, setPendingFiles] = useState<PendingFile[]>([]);
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const threadIdRef = useRef<string | null>(initialThreadId);
  const abortRef = useRef<AbortController | null>(null);
  // Synchronous guard against overlapping turns — a turn is strictly
  // one-at-a-time, never overlapping. `isSending` state alone isn't
  // enough: Composer already disables input while sending, but that
  // depends on React having committed the re-render; a state variable
  // read inside send()'s own closure would be exactly as stale. A ref
  // updates immediately, so this catches the same-tick race regardless
  // of render timing, not just the normal UI-gated path.
  const isSendingRef = useRef(false);
  // Set right when THIS hook's own send() learns a brand new thread_id
  // (see onThreadId below) — distinguishes "initialThreadId just changed
  // because our own navigate() caught up to a thread we already fully
  // know" from "initialThreadId changed because the user actually
  // navigated to a different thread's URL." Only the second case should
  // trigger a re-fetch; re-hydrating the first case would race a Redis
  // write that (per turn.rs) only happens once the stream finishes, and
  // would discard state that's already more current than anything Redis
  // has yet.
  const justCreatedRef = useRef(false);
  // deferred.md #8: whether the nudge has already fired for whichever
  // thread threadIdRef currently points at — reset below alongside
  // threadIdRef itself, on a genuine thread switch (not on the
  // "our own navigate() just caught up" branch, which is still the
  // same thread continuing forward).
  const hasNudgedRef = useRef(false);

  useEffect(() => {
    if (justCreatedRef.current && initialThreadId === threadIdRef.current) {
      justCreatedRef.current = false;
      return;
    }
    threadIdRef.current = initialThreadId;
    hasNudgedRef.current = false;
    // deferred.md #63: attachments/pendingFiles fell into the same
    // pre-existing hole composerNotice/isSending sit in — this effect
    // only ever reset messages/isHydrating, so a file staged for thread
    // A stayed visible above the composer after switching to thread B.
    // Reset here, once per genuine switch (this line already runs
    // exactly once per real switch, never on the justCreatedRef skip
    // branch above), not duplicated across each branch below.
    setAttachments([]);
    setPendingFiles([]);
    // A genuine switch always starts this hook fresh for the newly-
    // viewed thread, even if the OLD thread's send() is still running
    // in the background — left alone deliberately, not aborted, so it
    // finishes and persists normally (only cancel() aborts). This just
    // detaches this hook's own bookkeeping from it, so that old call's
    // eventual finally/callbacks no-op via the abortRef-identity check
    // they each carry, instead of stomping this fresh state once that
    // old request eventually settles.
    setIsSending(false);
    isSendingRef.current = false;
    abortRef.current = null;
    if (initialThreadId === null) {
      setMessages([]);
      setIsHydrating(false);
      return;
    }
    let cancelled = false;
    setIsHydrating(true);
    memorylessApi
      .getThread(initialThreadId)
      .then((history) => {
        if (cancelled) return;
        setMessages(history);
      })
      .catch(() => {
        // Expired/foreign/nonexistent thread — degrade to a fresh,
        // empty conversation under this same URL rather than erroring;
        // sending a message will simply start a new thread (the
        // backend's own load_owned already 404s on a bad thread_id).
        if (cancelled) return;
        threadIdRef.current = null;
        setMessages([]);
      })
      .finally(() => {
        if (!cancelled) setIsHydrating(false);
      });
    return () => {
      cancelled = true;
    };
  }, [initialThreadId]);

  // deferred.md #8: fires once per thread, whenever `messages` crosses
  // the threshold — covers both a live conversation (after a successful
  // turn adds messages) and a reloaded thread that already has enough
  // history on hydration, with one check instead of duplicating it at
  // both call sites. A turn that fails with no delta only adds the
  // student's own message back (see onError below), never the tutor's,
  // so a failed exchange correctly never counts toward the threshold.
  useEffect(() => {
    if (hasNudgedRef.current) return;
    if (Math.floor(messages.length / 2) < NUDGE_AFTER_EXCHANGES) return;
    const threadId = threadIdRef.current;
    if (!threadId) return;
    hasNudgedRef.current = true;
    setComposerNotice({
      tone: 'warning',
      message: 'You seem to be getting into this. Want to start a study thread?',
      actionLabel: 'Start a study thread',
      onAction: () => {
        setComposerNotice(null);
        onNudgeAccept?.(threadId);
      },
    });
  }, [messages, onNudgeAccept]);

  const send = useCallback(async (text: string) => {
    // See isSendingRef's own comment — a real, synchronous "is a turn
    // already running" check, not just relying on the UI having
    // disabled itself in time.
    if (isSendingRef.current) return;
    isSendingRef.current = true;

    setComposerNotice(null);
    // Found live: attachments never cleared on send, only the textarea
    // did — a sent message's attachment card sat in the composer
    // indefinitely. Cleared here, same optimistic timing as the
    // textarea's own immediate clear (Composer.tsx's handleSend).
    setAttachments([]);
    setPendingFiles([]);
    const studentMessage: ChatMessage = {
      id: `local-${Date.now()}`,
      role: 'student',
      text,
      timestamp: new Date().toISOString(),
    };
    const tutorMessageId = `local-tutor-${Date.now()}`;
    setMessages((prev) => [
      ...prev,
      studentMessage,
      { id: tutorMessageId, role: 'tutor', text: '', timestamp: new Date().toISOString() },
    ]);
    setIsSending(true);

    const controller = new AbortController();
    abortRef.current = controller;
    let receivedDelta = false;
    let sawError = false;

    try {
      await memorylessApi.streamMessage(
        { threadId: threadIdRef.current, message: text },
        {
          onThreadId: (id) => {
            // Abandoned call from a thread we've since switched away
            // from — without this, a late-arriving thread_id from a
            // still-finishing background request could yank the URL
            // back to it and corrupt threadIdRef for whatever the
            // now-current thread's own send() needs next.
            if (abortRef.current !== controller) return;
            const isNewThread = threadIdRef.current !== id;
            threadIdRef.current = id;
            if (isNewThread) {
              justCreatedRef.current = true;
              onThreadId?.(id);
            }
          },
          onDelta: (delta) => {
            receivedDelta = true;
            setMessages((prev) =>
              prev.map((m) => (m.id === tutorMessageId ? { ...m, text: m.text + delta } : m)),
            );
          },
          onError: (message) => {
            sawError = true;
            // deferred.md #53: this now fires for two real, different
            // cases — a transport-level failure (never any delta) and a
            // genuine backend-signaled `event: error` that can arrive
            // AFTER real content already streamed (e.g. a mid-generation
            // stall). Only strip the bubble in the first case; partial
            // text that already rendered is kept as the tutor's real
            // (partial) reply, matching the same "keep partial text"
            // philosophy already used for a user-initiated cancel
            // (turn.rs's own comment) — a server-side failure still
            // deserves a real signal, just not silently discarding
            // output that was already shown to be correct so far.
            if (!receivedDelta) {
              setMessages((prev) => prev.filter((m) => m.id !== tutorMessageId));
            }
            // Same abandoned-call guard as onThreadId above — an error
            // from a background-finishing request on a thread we've
            // switched away from shouldn't pop a notice over whatever
            // thread is now actually on screen.
            if (abortRef.current === controller) {
              setComposerNotice({ tone: 'error', message, actionLabel: 'Retry', onAction: () => send(text) });
            }
          },
        },
        controller.signal,
      );

      if (controller.signal.aborted) {
        // User-initiated stop — not a failure, no composerNotice. Marks
        // the placeholder (whatever text it has, empty or partial) as
        // interrupted; MessageBubble renders the real inline "response
        // was interrupted" + Try again UI for this, and `!interrupted`
        // stops `isMusing` from cycling forever on an empty placeholder
        // that will otherwise never receive a real delta or removal.
        setMessages((prev) =>
          prev.map((m) => (m.id === tutorMessageId ? { ...m, interrupted: true, promptText: text } : m)),
        );
      } else if (!receivedDelta && !sawError) {
        // Defensive backstop only, should rarely fire now: the backend
        // itself emits a real `event: error` for a mid-generation failure
        // as of deferred.md #53 (previously it didn't — the stream just
        // ended with zero deltas and a bare "done," indistinguishable from
        // an empty-but-successful reply, which is what this check was
        // originally built to catch). Kept in case some future failure
        // mode still ends the stream silently without an explicit error.
        setMessages((prev) => prev.filter((m) => m.id !== tutorMessageId));
        if (abortRef.current === controller) {
          setComposerNotice({
            tone: 'error',
            message: 'The tutor could not generate a response. Try again.',
            actionLabel: 'Retry',
            onAction: () => send(text),
          });
        }
      }
    } finally {
      // Only reset shared state if we still own it — a genuine thread
      // switch (see the effect above) already replaced abortRef.current
      // with a fresh value for whatever thread is now current; this
      // call's own completion should no-op in that case, not stomp it.
      if (abortRef.current === controller) {
        isSendingRef.current = false;
        setIsSending(false);
        abortRef.current = null;
      }
    }
  }, [onThreadId]);

  // Aborting the fetch IS the cancel signal the backend listens for
  // (see api/memoryless.ts's doc comment) — no separate endpoint call.
  // The actual "interrupted" UI (whatever text streamed in before the
  // stop stays on screen, matching industrial Claude/ChatGPT partial-
  // response behavior, plus a real inline notice + Try again) is handled
  // by send() itself once the aborted streamMessage() call settles — see
  // its `controller.signal.aborted` branch above — not here; only one
  // AbortController is ever live at a time (see the input-lock in
  // Composer, deferred.md's send/stop discussion), so there is nothing
  // else for this function itself to do.
  const cancel = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  const dismissComposerNotice = useCallback(() => setComposerNotice(null), []);

  // Shared by confirmAttachRole (first attempt) and retryAttachment (same
  // file, same role, tried again) — the only difference between those two
  // callers is where the Attachment entry already exists (retry) vs. still
  // needs creating (first attempt), so the actual upload+outcome handling
  // lives here once.
  const runUpload = useCallback(
    async (id: string, file: File, role: UploadRole) => {
      try {
        const result = await uploadsApi.uploadFile(file, role, threadIdRef.current, null);
        // Same reconciliation as send()'s onThreadId handler above: a
        // staged upload can create a brand-new thread just as easily as a
        // message can, if this is the very first thing the user does.
        if (result.threadId) {
          const isNewThread = threadIdRef.current !== result.threadId;
          threadIdRef.current = result.threadId;
          if (isNewThread) {
            justCreatedRef.current = true;
            onThreadId?.(result.threadId);
          }
        }
        setAttachments((prev) =>
          prev.map((a) =>
            a.id === id ? { ...a, status: 'ready', chunkCount: result.chunkCount, deduped: result.deduped } : a,
          ),
        );
      } catch (err) {
        setAttachments((prev) =>
          prev.map((a) =>
            a.id === id
              ? { ...a, status: 'error', errorMessage: err instanceof Error ? err.message : 'Upload failed.' }
              : a,
          ),
        );
      }
    },
    [onThreadId],
  );

  // Queues files the instant they're selected/dropped — no upload yet,
  // just makes them available for the role picker (Composer renders one
  // for pendingFiles[0] and advances through the rest sequentially, per
  // the "asked at upload time," one-modal-per-file decision).
  const requestAttach = useCallback((files: File[]) => {
    const queued = files.map((file) => ({ id: crypto.randomUUID(), file }));
    setPendingFiles((prev) => [...prev, ...queued]);
  }, []);

  // Fires once a role is picked for a queued file — moves it out of the
  // pending queue and into `attachments` as 'uploading', then kicks off
  // the real POST /uploads.
  const confirmAttachRole = useCallback(
    (pendingId: string, role: UploadRole) => {
      const pending = pendingFiles.find((p) => p.id === pendingId);
      if (!pending) return;
      setPendingFiles((prev) => prev.filter((p) => p.id !== pendingId));
      setAttachments((prev) => [...prev, { id: pending.id, file: pending.file, role, status: 'uploading' }]);
      void runUpload(pending.id, pending.file, role);
    },
    [pendingFiles, runUpload],
  );

  // Dismisses a file BEFORE its role was ever picked (e.g. closing the
  // modal) — nothing was uploaded, so this is a full, clean undo.
  const cancelPendingFile = useCallback((pendingId: string) => {
    setPendingFiles((prev) => prev.filter((p) => p.id !== pendingId));
  }, []);

  const retryAttachment = useCallback(
    (attachmentId: string) => {
      const attachment = attachments.find((a) => a.id === attachmentId);
      if (!attachment) return;
      setAttachments((prev) =>
        prev.map((a) => (a.id === attachmentId ? { ...a, status: 'uploading', errorMessage: undefined } : a)),
      );
      void runUpload(attachment.id, attachment.file, attachment.role);
    },
    [attachments, runUpload],
  );

  // UI-only: an already-uploaded staged attachment has no unstage/DELETE
  // endpoint, so dismissing its card doesn't remove it from the thread's
  // Redis blob — only hides it from this composer's own view.
  const removeAttachment = useCallback((attachmentId: string) => {
    setAttachments((prev) => prev.filter((a) => a.id !== attachmentId));
  }, []);

  return {
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
  };
}
