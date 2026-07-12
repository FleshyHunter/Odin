import { useCallback, useEffect, useState } from 'react';
import * as tracksApi from '../api/tracks';
import type { ChatMessage, Track } from '../types';

export function useTracks() {
  const [tracks, setTracks] = useState<Track[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    tracksApi.listTracks().then((result) => {
      if (cancelled) return;
      setTracks(result);
      setIsLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const removeTrack = useCallback(async (trackId: string) => {
    await tracksApi.deleteTrack(trackId);
    setTracks((prev) => prev.filter((t) => t.id !== trackId));
  }, []);

  const createTrack = useCallback(async (title: string) => {
    const track = await tracksApi.createTrack(title);
    setTracks((prev) => [...prev, track]);
    return track;
  }, []);

  const togglePin = useCallback(async (trackId: string) => {
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
