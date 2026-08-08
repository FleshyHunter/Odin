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
}

export type TrackLevel = 'Beginner' | 'Intermediate' | 'Advanced';
export type TrackGoal = 'Exam prep' | 'Project' | 'General understanding';

// Onboarding Diagnostic Step 1's structured intake (PRD.md; mirrors
// backend/src/ai_client::IntakeContext exactly) — null when the student
// used TrackModal's "Skip diagnostic" action instead of filling this in.
// deferred.md #40: TrackModal collects this for real, but createTrack's
// mock still just discards it (deferred.md #4's real backend isn't wired
// to the frontend yet).
export interface TrackIntake {
  level: TrackLevel;
  goal: TrackGoal;
  background: string | null;
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

export interface MatrixValue {
  rows: number;
  cols: number;
  values: Array<string | number>;
}

export interface Exercise {
  id: string;
  conceptTitle: string;
  difficulty: Difficulty;
  prompt: string;
  matrix?: MatrixValue;
  answerPlaceholder?: string;
}

export interface MasteryStatus {
  conceptTitle: string;
  masteryScore: number; // 0-1, mastery_bank.mastery_score
}

export interface SubmitAnswerResult {
  isCorrect: boolean;
  masteryScore: number;
}
