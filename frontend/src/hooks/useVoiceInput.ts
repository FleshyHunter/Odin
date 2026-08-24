import { useCallback, useRef, useState } from 'react';
import { streamTranscription, transcribeAudio, uploadTranscriptionChunk } from '../api/voice';

export type VoiceInputStatus = 'idle' | 'recording' | 'transcribing';

// Real implementation of the LOCKED voice input contract
// (ARCHITECTURE_LOCK.md, Upload System — Voice Input): mic tap ->
// MediaRecorder captures audio -> POST /voice/transcribe (FastAPI,
// Whisper base) -> { text } -> text drops into the Composer input box
// for the user to review -> user sends manually. Never auto-sent
// (Composer.tsx's own handleMicClick already enforces this — this hook
// only ever returns the transcript, it never touches the composer's
// own text/send state directly). deferred.md #80: previously a stub
// returning a hardcoded canned string via simulateDelay.
//
// Preference order matches what real browsers actually support via
// MediaRecorder.isTypeSupported — the first supported one wins;
// ai_service's own extraction (app/voice/service.py) already branches
// on whichever container arrives via the real filename's own
// extension, not a hardcoded assumption, so this doesn't need to match
// anything server-side.
const MIME_TYPE_PREFERENCE = ['audio/webm;codecs=opus', 'audio/webm', 'audio/mp4', 'audio/ogg;codecs=opus'];

// Chunked-streaming live captions (see api/voice.ts's streamTranscription/
// uploadTranscriptionChunk): re-transcribe the growing buffer roughly
// this often. Starting hypothesis, not a locked value — PRD.md's Voice
// Input section carries a dated annotation flagging this needs
// measuring under real concurrent-Ollama GPU load before being trusted
// long-term (base Whisper re-decoding a growing buffer shares the same
// GPU chat generation uses).
const CHUNK_INTERVAL_MS = 4000;
// If the SSE session doesn't hand back a session id within this long,
// proceed without live captions rather than blocking the mic from
// starting — the final authoritative transcribe() call on stop doesn't
// depend on this at all, so live captions are a pure bonus, never a
// dependency.
const SESSION_OPEN_TIMEOUT_MS = 2500;

// deferred.md #98 — a hard guardrail, not a fix for the growing-buffer
// cost problem itself (see #98's own entry): re-transcribing the whole
// recording every ~4s gets slower the longer it runs, and this is the
// backstop that keeps a runaway recording from degrading indefinitely.
// 90s is generous for a spoken question into a chat composer — nobody
// reasonably needs several continuous minutes of dictation here.
export const MAX_RECORDING_DURATION_MS = 90_000;

function pickSupportedMimeType(): string | null {
  if (typeof MediaRecorder === 'undefined') return null;
  return MIME_TYPE_PREFERENCE.find((type) => MediaRecorder.isTypeSupported(type)) ?? null;
}

function extensionFor(mimeType: string): string {
  if (mimeType.startsWith('audio/webm')) return 'webm';
  if (mimeType.startsWith('audio/mp4')) return 'mp4';
  if (mimeType.startsWith('audio/ogg')) return 'ogg';
  return 'webm';
}

export function useVoiceInput() {
  const [status, setStatus] = useState<VoiceInputStatus>('idle');
  // Scoped down deliberately (per plan): no retry affordance, no
  // dedicated error component — Composer.tsx just renders this as a
  // small inline message near the mic button.
  const [error, setError] = useState<string | null>(null);
  // Live, best-effort transcript of the recording so far, only
  // meaningful while status === 'recording'. This hook doesn't touch
  // the composer's own text state directly (Composer.tsx owns that) —
  // it just exposes this value, and Composer writes it straight into
  // its live text as it updates. deferred.md #98: this now accumulates
  // (each server tick sends only the NEW words, already diffed against
  // its own previous window server-side — see openStreamingSession's
  // own onPartial handler), it does not replace on every tick anymore.
  const [partialTranscript, setPartialTranscript] = useState('');
  const recorderRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const mimeTypeRef = useRef<string>('audio/webm');
  const sessionIdRef = useRef<string | null>(null);
  const sseAbortRef = useRef<AbortController | null>(null);
  // Self-throttle: if a Whisper pass is running behind, skip ticks
  // rather than piling up overlapping calls — the next successful tick
  // just picks up the larger buffer, including whatever was skipped.
  const chunkInFlightRef = useRef(false);

  const releaseStream = useCallback(() => {
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
  }, []);

  const maybeUploadChunk = useCallback(async () => {
    if (chunkInFlightRef.current || !sessionIdRef.current) return;
    chunkInFlightRef.current = true;
    try {
      const blob = new Blob(chunksRef.current, { type: mimeTypeRef.current });
      await uploadTranscriptionChunk(blob, `recording.${extensionFor(mimeTypeRef.current)}`, sessionIdRef.current);
    } catch {
      // Best-effort — one missed partial isn't fatal.
    } finally {
      chunkInFlightRef.current = false;
    }
  }, []);

  const openStreamingSession = useCallback((): Promise<void> => {
    return new Promise((resolve) => {
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        resolve();
      };
      const timeoutId = setTimeout(finish, SESSION_OPEN_TIMEOUT_MS);
      const controller = new AbortController();
      sseAbortRef.current = controller;

      streamTranscription(
        {
          onSessionId: (id) => {
            sessionIdRef.current = id;
            clearTimeout(timeoutId);
            finish();
          },
          // deferred.md #98 — the backend now sends only the NEW words
          // each tick (a diff against its own previous windowed
          // transcript), not the full transcript-so-far, so this
          // accumulates rather than replaces — matches how chat's own
          // onDelta already works. See the backend's diff_new_suffix
          // for why: re-transcribing the whole growing recording every
          // tick is what caused live captions to visibly freeze/slow
          // down the longer a recording ran.
          onPartial: (delta) => setPartialTranscript((prev) => (prev ? `${prev} ${delta}` : delta)),
          onError: () => {
            // Best-effort feature — a streaming failure doesn't
            // interrupt recording, it just means no live captions for
            // this take.
            clearTimeout(timeoutId);
            finish();
          },
        },
        controller.signal,
      ).catch(() => {});
    });
  }, []);

  const startRecording = useCallback(async () => {
    setError(null);
    setPartialTranscript('');
    const mimeType = pickSupportedMimeType();
    if (!mimeType) {
      setError('Voice input is not supported in this browser.');
      return;
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      streamRef.current = stream;
      mimeTypeRef.current = mimeType;
      chunksRef.current = [];
      sessionIdRef.current = null;

      await openStreamingSession();

      const recorder = new MediaRecorder(stream, { mimeType });
      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) chunksRef.current.push(event.data);
        void maybeUploadChunk();
      };
      recorderRef.current = recorder;
      recorder.start(CHUNK_INTERVAL_MS);
      setStatus('recording');
    } catch {
      // Permission denied, no available device, or getUserMedia
      // unsupported (insecure context) — all surface identically here.
      setError('Could not access the microphone. Check your browser permissions.');
      releaseStream();
    }
  }, [maybeUploadChunk, openStreamingSession, releaseStream]);

  const stopRecording = useCallback(async (): Promise<string> => {
    const recorder = recorderRef.current;
    sseAbortRef.current?.abort();
    sseAbortRef.current = null;
    sessionIdRef.current = null;
    if (!recorder || recorder.state === 'inactive') {
      setStatus('idle');
      setPartialTranscript('');
      return '';
    }
    setStatus('transcribing');

    const blob = await new Promise<Blob>((resolve) => {
      recorder.onstop = () => resolve(new Blob(chunksRef.current, { type: mimeTypeRef.current }));
      recorder.stop();
    });
    releaseStream();
    recorderRef.current = null;

    // Always a fresh authoritative call here, never the last streamed
    // partial: the last partial may be based on a slightly stale buffer
    // if a chunk upload was still in flight at stop time, and by now
    // Whisper/CUDA are already warm from the preceding chunk calls, so
    // this costs nothing extra. See api/voice.ts's own comment.
    try {
      const text = await transcribeAudio(blob, `recording.${extensionFor(mimeTypeRef.current)}`);
      setStatus('idle');
      setPartialTranscript('');
      return text;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not transcribe audio.');
      setStatus('idle');
      setPartialTranscript('');
      return '';
    }
  }, [releaseStream]);

  const cancelRecording = useCallback(() => {
    sseAbortRef.current?.abort();
    sseAbortRef.current = null;
    sessionIdRef.current = null;
    setPartialTranscript('');
    if (recorderRef.current && recorderRef.current.state !== 'inactive') {
      recorderRef.current.stop();
    }
    recorderRef.current = null;
    releaseStream();
    setStatus('idle');
  }, [releaseStream]);

  return { status, error, partialTranscript, startRecording, stopRecording, cancelRecording };
}
