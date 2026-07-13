import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { useVoiceInput } from '../../../hooks/useVoiceInput';
import './composer.css';

interface ComposerProps {
  onSend: (text: string) => void;
  disabled?: boolean;
  isSending?: boolean;
}

export function Composer({ onSend, disabled, isSending }: ComposerProps) {
  const [value, setValue] = useState('');
  const { status, startRecording, stopRecording } = useVoiceInput();
  const textareaRef = useRef<HTMLTextAreaElement>(null);

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
    if (isSending) return;
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
      startRecording();
      return;
    }
    if (status === 'recording') {
      const transcribed = await stopRecording();
      // Locked Voice Input UX: transcribed text drops into the input box
      // for the user to review/edit — never auto-sent (ARCHITECTURE_LOCK.md,
      // Upload System — Voice Input, step 6).
      setValue((prev) => (prev ? `${prev} ${transcribed}` : transcribed));
    }
  };

  return (
    <div className="composer">
      <div className="composer-box">
        <textarea
          ref={textareaRef}
          className="composer-textarea"
          rows={1}
          placeholder="Ask a question, or say what you're stuck on..."
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={handleKeyDown}
          disabled={disabled}
        />

        <div className="composer-toolbar">
          <div className="composer-toolbar-left">
            {/* No defined behavior yet (Claude's reference opens an
                attachment/tools menu) — placeholder affordance only. */}
            <button className="icon-btn" aria-label="Add" disabled={disabled}>
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
              disabled={status === 'transcribing' || disabled}
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
      </div>
    </div>
  );
}
