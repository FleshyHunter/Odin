import { useCallback, useEffect, useRef, useState } from 'react';
import * as memorylessApi from '../api/memoryless';
import * as uploadsApi from '../api/uploads';
import type { Attachment, ChatMessage, ComposerNoticeData, PendingFile, UploadRole } from '../types';

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
export function useMemorylessChat(initialThreadId: string | null = null, onThreadId?: (threadId: string) => void) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isSending, setIsSending] = useState(false);
  const [isHydrating, setIsHydrating] = useState(initialThreadId !== null);
  const [composerNotice, setComposerNotice] = useState<ComposerNoticeData | null>(null);
  const [pendingFiles, setPendingFiles] = useState<PendingFile[]>([]);
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const threadIdRef = useRef<string | null>(initialThreadId);
  const abortRef = useRef<AbortController | null>(null);
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

  useEffect(() => {
    if (justCreatedRef.current && initialThreadId === threadIdRef.current) {
      justCreatedRef.current = false;
      return;
    }
    threadIdRef.current = initialThreadId;
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

  const send = useCallback(async (text: string) => {
    setComposerNotice(null);
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
            setComposerNotice({ tone: 'error', message, actionLabel: 'Retry', onAction: () => send(text) });
          },
        },
        controller.signal,
      );

      // Defensive backstop only, should rarely fire now: the backend
      // itself emits a real `event: error` for a mid-generation failure
      // as of deferred.md #53 (previously it didn't — the stream just
      // ended with zero deltas and a bare "done," indistinguishable from
      // an empty-but-successful reply, which is what this check was
      // originally built to catch). Kept in case some future failure
      // mode still ends the stream silently without an explicit error.
      if (!receivedDelta && !sawError && !controller.signal.aborted) {
        setMessages((prev) => prev.filter((m) => m.id !== tutorMessageId));
        setComposerNotice({
          tone: 'error',
          message: 'The tutor could not generate a response. Try again.',
          actionLabel: 'Retry',
          onAction: () => send(text),
        });
      }
    } finally {
      setIsSending(false);
      abortRef.current = null;
    }
  }, [onThreadId]);

  // Aborting the fetch IS the cancel signal the backend listens for
  // (see api/memoryless.ts's doc comment) — no separate endpoint call.
  // Silent on purpose: a user-initiated stop isn't an error, so no
  // composerNotice; whatever text streamed in before the stop stays on
  // screen, matching industrial (Claude/ChatGPT) partial-response behavior.
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
        const result = await uploadsApi.uploadFile(file, role, threadIdRef.current);
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
