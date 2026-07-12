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
