import { Fragment, useEffect, useState, type AnchorHTMLAttributes, type ReactNode } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkBreaks from 'remark-breaks';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import type { ChatMessage } from '../../../types';
import { InterruptedNotice } from './InterruptedNotice';
import { MessageUtil } from './MessageUtil';
import 'katex/dist/katex.min.css';
import './messageBubble.css';

interface MessageBubbleProps {
  message: ChatMessage;
  // Only relevant for an `interrupted` tutor message — resends
  // `message.promptText`. Optional/unwired at call sites (like ChatView's
  // mocked track chat) that have no cancel path and so can never produce
  // an interrupted message in the first place.
  onRetry?: (text: string) => void;
}

// Shown in place of the empty placeholder tutor bubble (useMemorylessChat's
// send() adds one with text: '' the instant a message is sent, before any
// real delta has arrived) — raven-themed to match the mascot, not generic
// "Thinking..." text.
const MUSING_PHRASES = [
  // 1. Observation & Search (Starting broad)
  'Scanning the horizon…',
  'Peering through the dark…',
  'Sifting through the shadows…',
  'Sorting the glimmers…',

  // 2. Gathering & Processing (Deepening thought)
  'Gathering whispers…',
  'Weaving the whispers…',
  'Scrying…',
  'Musing…',

  // 3. Resting / Settling into the Answer (Refining)
  'Preening thought…',
  'Perching in thought…',
];

const MUSING_INTERVAL_MS = 2200;

// Cycles through MUSING_PHRASES while `active`; always restarts from the
// first phrase for a NEW wait rather than continuing wherever a previous
// one left off. Stops (interval cleared) the instant `active` goes false —
// i.e. the moment a real delta arrives and message.text stops being ''.
function useMusingPhrase(active: boolean): string {
  const [index, setIndex] = useState(0);
  useEffect(() => {
    if (!active) return;
    setIndex(0);
    const id = window.setInterval(() => {
      setIndex((prev) => (prev + 1) % MUSING_PHRASES.length);
    }, MUSING_INTERVAL_MS);
    return () => window.clearInterval(id);
  }, [active]);
  return MUSING_PHRASES[index];
}

// A plain <a> from react-markdown would navigate the whole SPA away from
// the chat instead of just opening the cited page — the tutor may well
// cite an external source.
function MarkdownLink({ href, children }: AnchorHTMLAttributes<HTMLAnchorElement>) {
  return (
    <a href={href} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  );
}

// The actual fix: tutor text is real markdown from the LLM (bold, lists,
// code, etc.) that was previously just displayed as literal text with
// the raw ** / - / ``` characters still in it. remark-gfm adds tables/
// strikethrough/autolinks on top of CommonMark; remark-breaks turns a
// single '\n' into a hard line break (CommonMark's own default treats
// one bare newline as just a space, joining lines into one paragraph) —
// matches this app's prior behavior of always breaking on '\n'. remark-
// math + rehype-katex render the LaTeX ($...$ inline, $$...$$ block)
// the system prompt now explicitly asks qwen to use for math notation
// (ai_service/app/generation/router.py) — qwen was already reaching for
// LaTeX unprompted before this existed, it just had nowhere to render;
// this also makes the bespoke active-panel MatrixRenderer redundant
// (matrices are just one case of what this now handles generally).
function renderTutorMarkdown(text: string): ReactNode {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm, remarkBreaks, remarkMath]}
      rehypePlugins={[rehypeKatex]}
      components={{ a: MarkdownLink }}
    >
      {text}
    </ReactMarkdown>
  );
}

// Tutor text may ALSO contain a lightweight `{{term}}` marker for the
// glow-highlighted vocabulary term seen in odin_session.html
// (<em class="term">eigenvalues</em>) — a custom Odin convention, not
// standard markdown, and not currently emitted by any real caller
// (confirmed: no backend/ai_service code produces it, no mock content
// uses it either — a documented-but-dormant capability inherited from
// the original prototype). Split on it FIRST, across the whole text
// (not per-line — a per-line split would break multi-line markdown like
// fenced code blocks or lists), then run each surrounding segment
// through real markdown independently. Known, accepted limitation given
// the above: a {{term}} marker landing INSIDE a single markdown span
// (e.g. mid-bold) would break that span across the two resulting
// segments — acceptable since nothing real does this today.
function renderTutorText(text: string): ReactNode {
  const parts = text.split(/(\{\{[^}]+\}\})/g);
  return parts.map((part, index) => {
    const match = part.match(/^\{\{([^}]+)\}\}$/);
    if (match) {
      return (
        <em key={index} className="term">
          {match[1]}
        </em>
      );
    }
    return <Fragment key={index}>{renderTutorMarkdown(part)}</Fragment>;
  });
}

// Student's own typed text stays literal, plain text — matching Claude/
// ChatGPT's real behavior (only the assistant's output is markdown-
// rendered; what a person typed is shown back exactly as they typed it,
// asterisks and all, not reinterpreted).
function renderStudentText(text: string): ReactNode {
  return text.split('\n').map((line, index) => (
    <Fragment key={index}>
      {index > 0 && <br />}
      {line}
    </Fragment>
  ));
}

export function MessageBubble({ message, onRetry }: MessageBubbleProps) {
  // Hook runs unconditionally (Rules of Hooks) — `active` is simply false,
  // a harmless no-op, for a student message. `!message.interrupted`
  // matters even when text is still '': a cancelled turn should stop
  // cycling musing phrases immediately, not keep "thinking" forever just
  // because no delta ever arrived before the stop.
  const isMusing = message.role === 'tutor' && message.text === '' && !message.interrupted;
  const musingPhrase = useMusingPhrase(isMusing);

  if (message.role === 'student') {
    return (
      <div className="msg student">
        <div className="bubble">{renderStudentText(message.text)}</div>
        <MessageUtil side="user" text={message.text} timestamp={message.timestamp} />
      </div>
    );
  }

  return (
    <div className="msg tutor">
      <div className={isMusing ? 'bubble-avatar musing' : 'bubble-avatar'}>
        <img src="/assets/odin-icon-large.png" alt="" aria-hidden="true" />
      </div>
      <div className="bubble">
        {isMusing ? (
          <span key={musingPhrase} className="musing-text">
            {musingPhrase}
          </span>
        ) : (
          renderTutorText(message.text)
        )}
        {message.interrupted && (
          <InterruptedNotice
            onRetry={message.promptText ? () => onRetry?.(message.promptText!) : undefined}
          />
        )}
      </div>
      {/* Shown once there's any real text, interrupted or not — the
          only case with nothing to copy yet is the still-musing empty
          placeholder, which isMusing being true already implies. */}
      {message.text !== '' && <MessageUtil side="ai" text={message.text} timestamp={message.timestamp} />}
    </div>
  );
}
