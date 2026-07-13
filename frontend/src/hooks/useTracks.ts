import { useCallback, useEffect, useRef, useState } from 'react';
import * as tracksApi from '../api/tracks';
import type { ChatMessage, Track } from '../types';

export function useTracks() {
  const [tracks, setTracks] = useState<Track[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  // Mutations await this so they always build on the initial load's base
  // list, even if a user acts before that load resolves. simulateDelay
  // snapshots its value at call time, not resolve time — without this,
  // a fast create/delete/pin racing the initial fetch would append to
  // whatever `tracks` happened to be at that instant (possibly still the
  // empty default), silently losing the base list, or the initial fetch
  // could resolve after the mutation and clobber it right back. Both
  // sides awaiting the same promise (attached in the order called)
  // guarantees the initial setTracks(result) always applies first.
  const initialLoadRef = useRef<Promise<Track[]> | null>(null);

  useEffect(() => {
    let cancelled = false;
    const promise = tracksApi.listTracks();
    initialLoadRef.current = promise;
    promise.then((result) => {
      if (cancelled) return;
      setTracks(result);
      setIsLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const removeTrack = useCallback(async (trackId: string) => {
    await initialLoadRef.current;
    await tracksApi.deleteTrack(trackId);
    setTracks((prev) => prev.filter((t) => t.id !== trackId));
  }, []);

  const createTrack = useCallback(async (title: string) => {
    await initialLoadRef.current;
    const track = await tracksApi.createTrack(title);
    setTracks((prev) => [...prev, track]);
    return track;
  }, []);

  const togglePin = useCallback(async (trackId: string) => {
    await initialLoadRef.current;
    const updated = await tracksApi.togglePin(trackId);
    setTracks((prev) => prev.map((t) => (t.id === trackId ? updated : t)));
  }, []);

  return { tracks, isLoading, removeTrack, createTrack, togglePin };
}

export function useTrackMessages(trackId: string) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isSending, setIsSending] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setIsLoading(true);
    tracksApi.getMessages(trackId).then((result) => {
      if (cancelled) return;
      setMessages(result);
      setIsLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [trackId]);

  const send = useCallback(
    async (text: string) => {
      const studentMessage: ChatMessage = {
        id: `local-${Date.now()}`,
        role: 'student',
        text,
        timestamp: new Date().toISOString(),
      };
      setMessages((prev) => [...prev, studentMessage]);
      setIsSending(true);
      try {
        const tutorReply = await tracksApi.sendMessage(trackId, text);
        setMessages((prev) => [...prev, tutorReply]);
      } finally {
        setIsSending(false);
      }
    },
    [trackId],
  );

  return { messages, isLoading, isSending, send };
}
