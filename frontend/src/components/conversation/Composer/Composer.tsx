import { useEffect, useRef, useState, type ChangeEvent, type KeyboardEvent } from 'react';
import { MAX_RECORDING_DURATION_MS, useVoiceInput } from '../../../hooks/useVoiceInput';
import { ComposerNotice } from './ComposerNotice';
import { AttachmentRow } from './AttachmentRow/AttachmentRow';
import { UploadRoleModal } from './UploadRoleModal/UploadRoleModal';
import type { Attachment, ComposerNoticeData, PendingFile, UploadRole } from '../../../types';
import './composer.css';

interface ComposerProps {
  onSend: (text: string) => void;
  disabled?: boolean;
  isSending?: boolean;
  // Called instead of onSend when the button is clicked while isSending
  // is true (the button already swaps to a stop icon in that state —
  // deferred.md #51 wired this so the click actually does something,
  // rather than the previous silent no-op).
  onStop?: () => void;
  notice?: ComposerNoticeData | null;
  onDismissNotice?: () => void;
  // deferred.md #37 (memoryless-only this pass — see plan doc). All
  // optional and only rendered when provided, so the track-mode call site
  // (ChatView.tsx, still fully mocked) stays exactly as it was — same
  // pattern already used for onStop above.
  attachments?: Attachment[];
  pendingFiles?: PendingFile[];
  onAttachFiles?: (files: File[]) => void;
  onConfirmAttachRole?: (pendingId: string, role: UploadRole) => void;
  onCancelPendingFile?: (pendingId: string) => void;
  onRemoveAttachment?: (attachmentId: string) => void;
  onRetryAttachment?: (attachmentId: string) => void;
  // deferred.md #92: memoryless-only opt-in — see handleSend's own
  // comment for why this can't just be inferred from attachments alone.
  allowEmptyTextWithAttachments?: boolean;
}

export function Composer({
  onSend,
  disabled,
  isSending,
  onStop,
  notice,
  onDismissNotice,
  attachments,
  pendingFiles,
  onAttachFiles,
  onConfirmAttachRole,
  onCancelPendingFile,
  onRemoveAttachment,
  onRetryAttachment,
  allowEmptyTextWithAttachments,
}: ComposerProps) {
  const [value, setValue] = useState('');
  const { status, error: voiceError, partialTranscript, startRecording, stopRecording, cancelRecording } =
    useVoiceInput();
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  // Whatever was already typed the moment recording started — live
  // partials get appended after this, never replacing it, so starting
  // voice input mid-draft doesn't blow away what's already there.
  const baseValueRef = useRef('');
  // deferred.md #98 — auto-stop guardrail. Owned here, not inside
  // useVoiceInput, because finishing a recording (applying the final
  // transcript to `value`) is Composer's own job either way — a timer
  // living in the hook would still need some way to hand the result
  // back up, so it's simpler to just have the timer trigger the exact
  // same finish path a manual mic-off click already uses.
  const maxDurationTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const attachEnabled = onAttachFiles !== undefined;
  const currentPendingFile = pendingFiles && pendingFiles.length > 0 ? pendingFiles[0] : null;

  // Auto-grow: starts at one row (matches the previous single-line input's
  // height), grows with content. The actual cap (50% of the .conversation
  // column's height) is enforced in CSS via max-height + overflow-y auto —
  // once scrollHeight exceeds that cap, the browser clips and this becomes
  // an internally scrollable box instead of growing further.
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${el.scrollHeight}px`;
  }, [value]);

  // Live transcript writes straight into the real composer text while
  // recording. partialTranscript (useVoiceInput.ts) already accumulates
  // correctly on its own — deferred.md #98: the backend only
  // re-transcribes a bounded trailing window each tick, not the whole
  // growing recording, and diffs it server-side so only genuinely new
  // words arrive here — so this effect just needs to combine it with
  // whatever was already typed before the mic was pressed (baseValueRef,
  // never touched by voice input) on every change.
  useEffect(() => {
    if (status !== 'recording') return;
    const base = baseValueRef.current;
    setValue(base ? `${base} ${partialTranscript}` : partialTranscript);
  }, [partialTranscript, status]);

  // Clear the max-duration timer on unmount so it can't fire against a
  // gone component.
  useEffect(() => {
    return () => {
      if (maxDurationTimeoutRef.current !== null) clearTimeout(maxDurationTimeoutRef.current);
    };
  }, []);

  const clearMaxDurationTimeout = () => {
    if (maxDurationTimeoutRef.current !== null) {
      clearTimeout(maxDurationTimeoutRef.current);
      maxDurationTimeoutRef.current = null;
    }
  };

  // Shared by a manual mic-off click and the max-duration auto-stop
  // (deferred.md #98) — same finish behavior either way, the student
  // just didn't have to press the button themselves in the timeout case.
  const finishRecording = async () => {
    clearMaxDurationTimeout();
    const transcribed = await stopRecording();
    const base = baseValueRef.current;
    // Locked Voice Input UX: transcribed text stays in the input box
    // for the user to review/edit — never auto-sent (ARCHITECTURE_LOCK.md,
    // Upload System — Voice Input, step 6). On failure (see
    // useVoiceInput's own error state, rendered separately below),
    // fall back to just the pre-recording base — the live partial that
    // had been showing was never authoritative.
    setValue(transcribed ? (base ? `${base} ${transcribed}` : transcribed) : base);
  };

  const handleSend = () => {
    if (isSending) {
      onStop?.();
      return;
    }
    // Sending while still recording: take whatever's currently shown
    // (already a live transcript, good enough to act on) and abort the
    // recording outright — no extra final-transcribe round-trip, that
    // would only add latency for text already sitting in the box.
    if (status === 'recording') {
      clearMaxDurationTimeout();
      cancelRecording();
    }
    const trimmed = value.trim();
    // deferred.md #92: empty text used to always block sending — now
    // valid for memoryless mode as long as there's at least one
    // attachment riding along (the tutor looks at the extracted content
    // and responds to that instead). Gated on the explicit
    // allowEmptyTextWithAttachments prop, NOT just "attachments is
    // non-empty" — journey mode (ChatView.tsx/useJourneyChat.ts) also
    // passes real attachments through this same prop, but its backend
    // (journeys/handlers.rs's send_journey_message) still hard-rejects
    // empty text; without this gate, an empty send would reach it and
    // surface a confusing validation error there.
    if (!trimmed && !(allowEmptyTextWithAttachments && attachments && attachments.length > 0)) return;
    onSend(trimmed);
    setValue('');
  };

  // Manually editing the composer while voice is live is treated as an
  // explicit "I'll take it from here" — recording stops instantly
  // rather than fighting the next tick's re-transcription over the
  // same text. The user's own edit (already applied to event.target.value
  // by the browser) is kept as-is; recording just stops contributing.
  const handleTextareaChange = (event: ChangeEvent<HTMLTextAreaElement>) => {
    if (status === 'recording') {
      clearMaxDurationTimeout();
      cancelRecording();
    }
    setValue(event.target.value);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      handleSend();
    }
    // Shift+Enter falls through to the textarea's default behavior — inserts a newline.
  };

  const handleMicClick = async () => {
    if (status === 'idle') {
      baseValueRef.current = value;
      // Not awaited — getUserMedia()/permission prompt resolves
      // asynchronously; the hook's own status/error state drives the UI
      // from here, this click handler doesn't need to wait on it.
      void startRecording();
      // deferred.md #98 — hard guardrail against the growing-buffer
      // cost problem (see that entry). void: finishRecording() already
      // handles its own state, nothing here needs to await it.
      maxDurationTimeoutRef.current = setTimeout(() => {
        void finishRecording();
      }, MAX_RECORDING_DURATION_MS);
      return;
    }
    if (status === 'recording') {
      await finishRecording();
    }
  };

  const handleAttachClick = () => {
    fileInputRef.current?.click();
  };

  const handleFileInputChange = (event: ChangeEvent<HTMLInputElement>) => {
    const files = event.target.files;
    if (files && files.length > 0) {
      onAttachFiles?.(Array.from(files));
    }
    // Reset so selecting the exact same file again still fires onChange.
    event.target.value = '';
  };

  const frameClassName = notice
    ? `composer-frame composer-frame-with-notice composer-frame-${notice.tone}`
    : 'composer-frame';

  return (
    <div className="composer">
      {attachEnabled && (
        <UploadRoleModal
          file={currentPendingFile?.file ?? null}
          onSelectRole={(role) => currentPendingFile && onConfirmAttachRole?.(currentPendingFile.id, role)}
          onClose={() => currentPendingFile && onCancelPendingFile?.(currentPendingFile.id)}
        />
      )}
      <div className={frameClassName}>
        {notice && (
          <ComposerNotice
            tone={notice.tone}
            message={notice.message}
            actionLabel={notice.actionLabel}
            onAction={notice.onAction}
            onDismiss={onDismissNotice ?? (() => {})}
          />
        )}
        <div className="composer-box">
            {attachEnabled && attachments && attachments.length > 0 && (
              <AttachmentRow
                attachments={attachments}
                onRemove={(id) => onRemoveAttachment?.(id)}
                onRetry={(id) => onRetryAttachment?.(id)}
              />
            )}
            <textarea
              ref={textareaRef}
              className="composer-textarea"
              rows={1}
              placeholder="Ask a question, or say what you're stuck on..."
              value={value}
              onChange={handleTextareaChange}
              onKeyDown={handleKeyDown}
              // Stays editable while isSending — a turn is still
              // strictly one-at-a-time server-side (handleSend's own
              // isSending branch routes Enter/click to onStop, not a
              // second send), but the student can keep drafting their
              // next message while a reply streams in, same as
              // ChatGPT/Claude's own composer behavior.
              disabled={disabled}
            />

            <div className="composer-toolbar">
              <div className="composer-toolbar-left">
                {attachEnabled && (
                  <input
                    ref={fileInputRef}
                    type="file"
                    multiple
                    hidden
                    onChange={handleFileInputChange}
                  />
                )}
                <button
                  className="icon-btn"
                  aria-label="Add"
                  onClick={attachEnabled ? handleAttachClick : undefined}
                  disabled={disabled || !attachEnabled || isSending}
                >
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8}>
                    <line x1="12" y1="5" x2="12" y2="19" />
                    <line x1="5" y1="12" x2="19" y2="12" />
                  </svg>
                </button>
              </div>

              <div className="composer-toolbar-right">
                <button
                  className="icon-btn"
                  aria-label={status === 'recording' ? 'Stop recording' : 'Voice input'}
                  aria-pressed={status === 'recording'}
                  onClick={handleMicClick}
                  // Blocks STARTING a new recording while a turn is
                  // sending (voice shares the same GPU as generation),
                  // but never strands an already-active recording with
                  // no way to stop it just because a separate text send
                  // started/finished mid-recording.
                  disabled={status === 'transcribing' || disabled || (isSending && status !== 'recording')}
                >
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7}>
                    <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" />
                    <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
                    <line x1="12" y1="19" x2="12" y2="23" />
                  </svg>
                </button>

                <button
                  className="send-btn"
                  aria-label={isSending ? 'Stop' : 'Send'}
                  onClick={handleSend}
                  disabled={disabled}
                >
                  {isSending ? (
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
                      <rect x="3" y="3" width="18" height="18" rx="4" />
                    </svg>
                  ) : (
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.2}>
                      <line x1="12" y1="19" x2="12" y2="5" />
                      <polyline points="5 12 12 5 19 12" />
                    </svg>
                  )}
                </button>
              </div>
            </div>
            {/* deferred.md #80 — scoped down deliberately: no retry
                affordance, no dedicated error component, just a small
                inline message near the mic button that caused it. */}
            {voiceError && <p className="composer-voice-error">{voiceError}</p>}
          </div>
      </div>
    </div>
  );
}
