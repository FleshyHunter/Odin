import { useCallback, useEffect, useRef, useState } from 'react';
import * as tracksApi from '../api/tracks';
import type { Track } from '../types';

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

  const createTrackFromJourney = useCallback(async (title: string, journeyId: string) => {
    await initialLoadRef.current;
    const track = await tracksApi.createTrackFromJourney(title, journeyId);
    setTracks((prev) => [...prev, track]);
    return track;
  }, []);

  const togglePin = useCallback(async (trackId: string) => {
    await initialLoadRef.current;
    const updated = await tracksApi.togglePin(trackId);
    setTracks((prev) => prev.map((t) => (t.id === trackId ? updated : t)));
  }, []);

  const renameTrack = useCallback(async (trackId: string, title: string) => {
    await initialLoadRef.current;
    const updated = await tracksApi.renameTrack(trackId, title);
    setTracks((prev) => prev.map((t) => (t.id === trackId ? updated : t)));
  }, []);

  const setTrackProject = useCallback(async (trackId: string, projectId: string | null) => {
    await initialLoadRef.current;
    const updated = await tracksApi.setTrackProject(trackId, projectId);
    setTracks((prev) => prev.map((t) => (t.id === trackId ? updated : t)));
  }, []);

  return { tracks, isLoading, removeTrack, createTrackFromJourney, togglePin, renameTrack, setTrackProject };
}
