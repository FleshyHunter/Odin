export type UserId = string;

export interface User {
  userId: UserId;
  displayName: string;
  email: string;
}

export interface AuthSession {
  token: string;
  user: User;
  expiresAt: string; // ISO date, 30-day JWT expiry per ARCHITECTURE_LOCK.md Auth section
}

export type TrackStatus = 'active' | 'paused' | 'completed';

// "Track" is the UI-facing name for a journey (ARCHITECTURE_LOCK.md: Subject vs Journey).
export interface Track {
  id: string;
  title: string;
  subjectTitle: string;
  currentConceptTitle: string | null; // null until a subject/concept exists for this track
  status: TrackStatus;
  pinned: boolean;
  projectId: string | null;
  lastActiveAt: string;
  // The real backend journey this (still otherwise fully mocked) Track
  // carries, from POST /journeys/start (deferred.md #4/#40) — null only
  // for the pre-existing seed/demo track, which predates real wiring.
  // Nothing else in the app reads this yet (journey listing/detail and
  // real track-mode teaching are still out of #4's scope), but it's
  // real, not a placeholder — carried through so it's there once
  // something does need it.
  journeyId: string | null;
}

export type TrackLevel = 'Beginner' | 'Intermediate' | 'Advanced';
export type TrackGoal = 'Exam prep' | 'Project' | 'General understanding';

// Onboarding Diagnostic Step 1's structured intake (PRD.md; mirrors
// backend/src/ai_client::IntakeContext exactly) — null when the student
// used TrackModal's "Skip diagnostic" action instead of filling this in.
// deferred.md #4/#40: real, wired end-to-end — TrackModal's wizard sends
// this to POST /journeys/start via api/journeys.ts.
export interface TrackIntake {
  level: TrackLevel;
  goal: TrackGoal;
  background: string | null;
}

// Onboarding Diagnostic Steps 2-3 (PRD.md) — a real question to answer,
// from POST /journeys/start or a backup question surfaced on
// contradiction. `choices` present means multiple-choice; absent means a
// free-text answer.
export interface DiagnosticQuestion {
  diagnosticId: string;
  question: string;
  exerciseType: string;
  choices: string[] | null;
}

// Contract: exactly one of `diagnostic`/`journeyId` is set — `journeyId`
// when the intake was skipped (journey created immediately, no
// diagnostic needed), `diagnostic` otherwise (answer it via
// POST /journeys/diagnostic/:id/respond).
export interface JourneyStartResult {
  diagnostic: DiagnosticQuestion | null;
  journeyId: string | null;
}

// Same shape as DiagnosticQuestion minus diagnosticId — the backup
// question is answered against the SAME diagnostic_id via retry-backup,
// not a new one.
export interface BackupQuestion {
  question: string;
  exerciseType: string;
  choices: string[] | null;
}

// Response from /respond, /retry-backup, /confirm-downgrade. journeyId
// set means done (either confirmed or downgraded); contradicted + a
// backupQuestion means PRD.md Step 3's disclosure UI should show,
// offering Confirm or Try backup question.
export interface DiagnosticOutcome {
  contradicted: boolean;
  backupQuestion: BackupQuestion | null;
  journeyId: string | null;
}

export interface Project {
  id: string;
  title: string;
  description: string | null;
  updatedAt: string;
}

export type MessageRole = 'tutor' | 'student';

export interface ChatMessage {
  id: string;
  role: MessageRole;
  text: string;
  timestamp: string;
  // Set on a tutor message whose turn was cancelled by the user (not a
  // failure) — MessageBubble renders a distinct "response was
  // interrupted" container instead of (or after, if some text already
  // streamed in) the normal bubble content. `promptText` is the
  // original student message that produced this turn, carried directly
  // on the message so "Try again" can resend it without depending on
  // message-list ordering to find it.
  interrupted?: boolean;
  promptText?: string;
}

// A graceful, dismissible banner that attaches to the top of the
// Composer (see ComposerNotice) — 'warning' for advisory/non-blocking
// conditions (e.g. a future rate limit), 'error' for something that
// actually failed (e.g. the tutoring engine being unreachable).
export type ComposerNoticeTone = 'warning' | 'error';

export interface ComposerNoticeData {
  tone: ComposerNoticeTone;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
}

// Full union kept here to match the backend's VALID_ROLES (backend/src/
// uploads/handlers.rs) exactly, even though only 'prompt_upload'/
// 'material_upload' are reachable via the UI right now — 'ephemeral' is
// deliberately not wired yet (deferred.md #37: no message-level field
// exists in POST /memoryless/messages to carry one-turn-only context).
export type UploadRole = 'ephemeral' | 'prompt_upload' | 'material_upload';
export type AttachmentStatus = 'uploading' | 'ready' | 'error';

// One in-flight or completed upload attached to the composer. Lives only
// in the composer's own local state — the backend's source of truth for a
// staged upload is the thread's Redis blob (see backend/src/memoryless/
// staging.rs's StagedUpload), not this shape.
export interface Attachment {
  id: string; // local id, stable across the upload lifecycle
  file: File; // kept for image object-URL preview + retry
  role: UploadRole;
  status: AttachmentStatus;
  errorMessage?: string;
  chunkCount?: number;
  deduped?: boolean;
}

// A file selected/dropped but not yet role-picked — shared between
// useMemorylessChat.ts (owns the queue) and Composer.tsx (renders the
// role-picker modal for pendingFiles[0]).
export interface PendingFile {
  id: string;
  file: File;
}

export type Difficulty = 'basic' | 'intermediate' | 'advanced';

export interface Exercise {
  id: string;
  conceptTitle: string;
  difficulty: Difficulty;
  // Short display name ("Matrix Transpose"), distinct from `prompt` (the
  // actual question text) — no real backend field for this yet (exercises
  // only has template_body/question_template), so it's mocked here for
  // now; persisting it for real is separate, later backend work (likely
  // Dify-generated alongside the template).
  title: string;
  prompt: string;
  answerPlaceholder?: string;
}

export interface MasteryStatus {
  conceptTitle: string;
  masteryScore: number; // 0-1, mastery_bank.mastery_score
}

export interface SubmitAnswerResult {
  isCorrect: boolean;
  masteryScore: number;
  // Mocked ahead of the real backend contract — mirrors quiz_attempts'
  // real expected_answer/feedback columns (SCHEMA.md), not invented from
  // nothing. Powers ExerciseCard's Answer toggle.
  expectedAnswer?: string;
  feedback?: string;
}

// One past attempt at a concept's exercise — mirrors quiz_attempts'
// real shape (rendered_question, is_correct, difficulty_attempted,
// timestamp), scoped down to what the history list actually displays.
export interface Attempt {
  id: string;
  title: string;
  question: string;
  isCorrect: boolean;
  date: string;
  difficulty: Difficulty;
}
