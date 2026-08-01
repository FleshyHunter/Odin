import { useCallback, useRef, useState } from 'react';
import * as memorylessApi from '../api/memoryless';
import type { ChatMessage, ComposerNoticeData } from '../types';

// Real memoryless chat (deferred.md #51) — the /chat landing composer's
// previous onSend={() => {}} no-op replaced with the actual backend
// turn (POST /memoryless/messages, streamed). thread_id lives in a ref,
// not state: it's an internal bookkeeping detail (which Redis-staged
// thread to keep appending to) that no render needs to react to.
export function useMemorylessChat() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isSending, setIsSending] = useState(false);
  const [composerNotice, setComposerNotice] = useState<ComposerNoticeData | null>(null);
  const threadIdRef = useRef<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);

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
            threadIdRef.current = id;
          },
          onDelta: (delta) => {
            receivedDelta = true;
            setMessages((prev) =>
              prev.map((m) => (m.id === tutorMessageId ? { ...m, text: m.text + delta } : m)),
            );
          },
          onError: (message) => {
            sawError = true;
            // No partial tutor bubble left behind for a turn that never
            // really produced anything — matches the empty-field/failed-
            // send handling already used across the auth forms.
            setMessages((prev) => prev.filter((m) => m.id !== tutorMessageId));
            setComposerNotice({ tone: 'error', message, actionLabel: 'Retry', onAction: () => send(text) });
          },
        },
        controller.signal,
      );

      // A turn that ai_service fails mid-generation on (confirmed live:
      // qwen's model server failing to start) doesn't surface as an SSE
      // error event at all — the backend's stream just ends with zero
      // deltas and a bare "done" (the failure reason only ever reaches
      // the Postgres-bound audit event, per memoryless/turn.rs). Without
      // this check, that leaves a permanently empty tutor bubble with no
      // indication anything went wrong.
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
  }, []);

  // Aborting the fetch IS the cancel signal the backend listens for
  // (see api/memoryless.ts's doc comment) — no separate endpoint call.
  // Silent on purpose: a user-initiated stop isn't an error, so no
  // composerNotice; whatever text streamed in before the stop stays on
  // screen, matching industrial (Claude/ChatGPT) partial-response behavior.
  const cancel = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  const dismissComposerNotice = useCallback(() => setComposerNotice(null), []);

  return { messages, isSending, send, cancel, composerNotice, dismissComposerNotice };
}
