import { useCallback, useEffect, useRef, useState } from 'react';
import * as journeyChatApi from '../api/journeyChat';
import * as uploadsApi from '../api/uploads';
import type { Attachment, ChatMessage, ComposerNoticeData, PendingFile, UploadRole } from '../types';

// Real journey-mode chat (deferred.md #2a) — mirrors useMemorylessChat.ts's
// shape closely, with one real difference: the tutor speaks first. On
// mount, this checks for an existing thread (GET); if none exists yet, it
// immediately triggers the tutor-initiated opening turn instead of
// waiting for the student to type something — matching Flow 4's "teach
// from first concept" and giving current_concept_id classification a
// real anchor from message one (see the plan's own reasoning, deferred.md
// #2a's entry).
export function useJourneyChat(journeyId: string | null) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isSending, setIsSending] = useState(false);
  const [isHydrating, setIsHydrating] = useState(journeyId !== null);
  const [composerNotice, setComposerNotice] = useState<ComposerNoticeData | null>(null);
  const [pendingFiles, setPendingFiles] = useState<PendingFile[]>([]);
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const threadIdRef = useRef<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  // deferred.md #79: mirrors useMemorylessChat.ts's own #76 fix — a
  // real, synchronous "is a turn already running" guard, not just
  // relying on Composer having disabled its input in time. Only send()
  // needs it; beginOpening has no user-facing trigger to race against.
  const isSendingRef = useRef(false);

  const beginOpening = useCallback(() => {
    if (!journeyId) return;
    const tutorMessageId = `local-opening-${Date.now()}`;
    setMessages([{ id: tutorMessageId, role: 'tutor', text: '', timestamp: new Date().toISOString() }]);
    setIsSending(true);
    setComposerNotice(null);

    const controller = new AbortController();
    abortRef.current = controller;
    let receivedDelta = false;

    journeyChatApi
      .startJourneyThread(
        journeyId,
        {
          onThreadId: (id) => {
            // Abandoned call from a journey we've since switched away
            // from — see the switch effect's own comment below.
            if (abortRef.current !== controller) return;
            threadIdRef.current = id;
          },
          onDelta: (delta) => {
            receivedDelta = true;
            setMessages((prev) => prev.map((m) => (m.id === tutorMessageId ? { ...m, text: m.text + delta } : m)));
          },
          onError: (message) => {
            if (!receivedDelta) {
              setMessages((prev) => prev.filter((m) => m.id !== tutorMessageId));
            }
            if (abortRef.current === controller) {
              setComposerNotice({ tone: 'error', message, actionLabel: 'Retry', onAction: beginOpening });
            }
          },
        },
        controller.signal,
      )
      .finally(() => {
        // Only reset shared state if we still own it — a genuine switch
        // (see the effect below) already replaced abortRef.current with
        // a fresh value for whatever journey is now current.
        if (abortRef.current === controller) {
          setIsSending(false);
          abortRef.current = null;
        }
      });
  }, [journeyId]);

  useEffect(() => {
    // A genuine switch (including switching to null) always starts this
    // hook fresh for whatever journey is now current, even if the OLD
    // journey's send()/beginOpening() is still running in the
    // background — left alone deliberately, not aborted, so it finishes
    // and persists normally. This just detaches this hook's own
    // bookkeeping from it, so that old call's eventual finally/callbacks
    // no-op via the abortRef-identity check they each carry, instead of
    // stomping this fresh state once that old request eventually
    // settles. beginOpening() below (on the "no thread yet" branch)
    // creates its own fresh controller synchronously after this, so
    // there's no window for interference.
    setIsSending(false);
    isSendingRef.current = false;
    abortRef.current = null;
    if (!journeyId) {
      setMessages([]);
      setIsHydrating(false);
      threadIdRef.current = null;
      return;
    }
    let cancelled = false;
    setIsHydrating(true);
    journeyChatApi
      .getJourneyThread(journeyId)
      .then((state) => {
        if (cancelled) return;
        if (state) {
          threadIdRef.current = state.threadId;
          setMessages(state.messages);
          setIsHydrating(false);
          return;
        }
        // No thread yet — the tutor-initiated opening IS the hydration
        // result in this case, not a separate empty state to render.
        setIsHydrating(false);
        beginOpening();
      })
      .catch(() => {
        if (cancelled) return;
        setIsHydrating(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- beginOpening intentionally excluded: it's stable per journeyId already, re-including it would re-run this on every render since useCallback's identity changes with journeyId anyway (same as journeyId itself, so no functional difference, just avoids double-listing the same real dependency).
  }, [journeyId]);

  const send = useCallback(
    async (text: string) => {
      if (!journeyId) return;
      // See isSendingRef's own comment.
      if (isSendingRef.current) return;
      isSendingRef.current = true;

      setComposerNotice(null);
      // Same fix as useMemorylessChat.ts's own send() — attachments
      // never cleared on send, only the textarea did.
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

      try {
        await journeyChatApi.sendJourneyMessage(
          journeyId,
          text,
          {
            onDelta: (delta) => {
              receivedDelta = true;
              setMessages((prev) =>
                prev.map((m) => (m.id === tutorMessageId ? { ...m, text: m.text + delta } : m)),
              );
            },
            onError: (message) => {
              if (!receivedDelta) {
                setMessages((prev) => prev.filter((m) => m.id !== tutorMessageId));
              }
              // Abandoned call from a journey we've since switched away
              // from — same guard as beginOpening's own callbacks.
              if (abortRef.current === controller) {
                setComposerNotice({ tone: 'error', message, actionLabel: 'Retry', onAction: () => send(text) });
              }
            },
          },
          controller.signal,
        );

        if (controller.signal.aborted) {
          // deferred.md #79: mirrors #76's fix exactly — mark the
          // placeholder interrupted (stops MessageBubble's musing
          // animation immediately, whether or not any text had already
          // streamed in) instead of leaving it to cycle "musing"
          // forever, and instead of the old silent no-op here.
          setMessages((prev) =>
            prev.map((m) => (m.id === tutorMessageId ? { ...m, interrupted: true, promptText: text } : m)),
          );
        } else if (!receivedDelta) {
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
        // Only reset shared state if we still own it — see the switch
        // effect's own comment.
        if (abortRef.current === controller) {
          isSendingRef.current = false;
          setIsSending(false);
          abortRef.current = null;
        }
      }
    },
    [journeyId],
  );

  const cancel = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  const dismissComposerNotice = useCallback(() => setComposerNotice(null), []);

  // deferred.md #37: mirrors useMemorylessChat.ts's own attachment state
  // machine exactly, simpler here since journeyId is already known
  // upfront — no "did this upload just create a brand-new thread"
  // reconciliation needed (uploadFile's journeyId branch never returns
  // a thread_id at all, see api/uploads.ts).
  const runUpload = useCallback(
    async (id: string, file: File, role: UploadRole) => {
      try {
        const result = await uploadsApi.uploadFile(file, role, null, journeyId);
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
    [journeyId],
  );

  const requestAttach = useCallback((files: File[]) => {
    const queued = files.map((file) => ({ id: crypto.randomUUID(), file }));
    setPendingFiles((prev) => [...prev, ...queued]);
  }, []);

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
