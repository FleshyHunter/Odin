import { useCallback, useEffect, useRef, useState } from 'react';
import * as journeyChatApi from '../api/journeyChat';
import type { ChatMessage, ComposerNoticeData } from '../types';

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
  const threadIdRef = useRef<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);

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
            setComposerNotice({ tone: 'error', message, actionLabel: 'Retry', onAction: beginOpening });
          },
        },
        controller.signal,
      )
      .finally(() => {
        setIsSending(false);
        abortRef.current = null;
      });
  }, [journeyId]);

  useEffect(() => {
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
              setComposerNotice({ tone: 'error', message, actionLabel: 'Retry', onAction: () => send(text) });
            },
          },
          controller.signal,
        );

        if (!receivedDelta && !controller.signal.aborted) {
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
    },
    [journeyId],
  );

  const cancel = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  const dismissComposerNotice = useCallback(() => setComposerNotice(null), []);

  return { messages, isSending, isHydrating, send, cancel, composerNotice, dismissComposerNotice };
}
