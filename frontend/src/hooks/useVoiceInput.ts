import { useCallback, useState } from 'react';
import { simulateDelay } from '../api/client';

export type VoiceInputStatus = 'idle' | 'recording' | 'transcribing';

// Stub matching the LOCKED voice input contract exactly (ARCHITECTURE_LOCK.md,
// Upload System — Voice Input):
//   mic tap -> MediaRecorder captures WebM/Opus -> POST /transcribe (FastAPI,
//   Whisper base) -> { text } -> text drops into the Composer input box for
//   the user to review -> user sends manually. Never auto-sent.
//
// Real implementation later: startRecording opens a MediaRecorder stream;
// stopRecording stops it, POSTs the resulting Blob to FastAPI's /transcribe,
// and returns response.text. The status state machine and return contract
// (Promise<string>) stay identical, so callers (Composer.tsx) don't change.
export function useVoiceInput() {
  const [status, setStatus] = useState<VoiceInputStatus>('idle');

  const startRecording = useCallback(() => {
    setStatus('recording');
  }, []);

  const stopRecording = useCallback(async (): Promise<string> => {
    setStatus('transcribing');
    const text = await simulateDelay('what is the determinant of a 2x2 matrix?', 900);
    setStatus('idle');
    return text;
  }, []);

  const cancelRecording = useCallback(() => {
    setStatus('idle');
  }, []);

  return { status, startRecording, stopRecording, cancelRecording };
}
