import { useCallback, useRef, useState } from 'react';
import { transcribeAudio } from '../api/voice';

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
  const recorderRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const mimeTypeRef = useRef<string>('audio/webm');

  const releaseStream = useCallback(() => {
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
  }, []);

  const startRecording = useCallback(async () => {
    setError(null);
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
      const recorder = new MediaRecorder(stream, { mimeType });
      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) chunksRef.current.push(event.data);
      };
      recorderRef.current = recorder;
      recorder.start();
      setStatus('recording');
    } catch {
      // Permission denied, no available device, or getUserMedia
      // unsupported (insecure context) — all surface identically here.
      setError('Could not access the microphone. Check your browser permissions.');
      releaseStream();
    }
  }, [releaseStream]);

  const stopRecording = useCallback(async (): Promise<string> => {
    const recorder = recorderRef.current;
    if (!recorder || recorder.state === 'inactive') {
      setStatus('idle');
      return '';
    }
    setStatus('transcribing');

    const blob = await new Promise<Blob>((resolve) => {
      recorder.onstop = () => resolve(new Blob(chunksRef.current, { type: mimeTypeRef.current }));
      recorder.stop();
    });
    releaseStream();
    recorderRef.current = null;

    try {
      const text = await transcribeAudio(blob, `recording.${extensionFor(mimeTypeRef.current)}`);
      setStatus('idle');
      return text;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not transcribe audio.');
      setStatus('idle');
      return '';
    }
  }, [releaseStream]);

  const cancelRecording = useCallback(() => {
    if (recorderRef.current && recorderRef.current.state !== 'inactive') {
      recorderRef.current.stop();
    }
    recorderRef.current = null;
    releaseStream();
    setStatus('idle');
  }, [releaseStream]);

  return { status, error, startRecording, stopRecording, cancelRecording };
}
