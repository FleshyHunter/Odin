import { useEffect, useRef, useState, type ChangeEvent, type KeyboardEvent } from 'react';
import { useVoiceInput } from '../../../hooks/useVoiceInput';
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
}: ComposerProps) {
  const [value, setValue] = useState('');
  const { status, error: voiceError, startRecording, stopRecording } = useVoiceInput();
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
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

  const handleSend = () => {
    if (isSending) {
      onStop?.();
      return;
    }
    const trimmed = value.trim();
    if (!trimmed) return;
    onSend(trimmed);
    setValue('');
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
      // Not awaited — getUserMedia()/permission prompt resolves
      // asynchronously; the hook's own status/error state drives the UI
      // from here, this click handler doesn't need to wait on it.
      void startRecording();
      return;
    }
    if (status === 'recording') {
      const transcribed = await stopRecording();
      // Locked Voice Input UX: transcribed text drops into the input box
      // for the user to review/edit — never auto-sent (ARCHITECTURE_LOCK.md,
      // Upload System — Voice Input, step 6). Empty on any failure (see
      // useVoiceInput's own error state, rendered separately below) —
      // nothing to insert in that case.
      if (transcribed) {
        setValue((prev) => (prev ? `${prev} ${transcribed}` : transcribed));
      }
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
              onChange={(event) => setValue(event.target.value)}
              onKeyDown={handleKeyDown}
              // isSending locks input (not just the send button, which
              // stays clickable below so it can still act as Stop) — a
              // turn is strictly one-at-a-time, never overlapping,
              // matching handleSend's own isSending branch above.
              disabled={disabled || isSending}
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
                  disabled={status === 'transcribing' || disabled || isSending}
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
