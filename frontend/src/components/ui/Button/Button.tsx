import type { ReactNode } from 'react';
import './button.css';

interface ButtonProps {
  type?: 'button' | 'submit';
  onClick?: () => void;
  disabled?: boolean;
  children: ReactNode;
}

// The one real shared button — the full-width gold CTA used for
// Sign in / Sign up / Continue / Submit. Consolidates what used to be
// two copy-pasted CSS classes (.primary in auth forms, .submit-btn in
// ExerciseCard) that had already drifted slightly (different padding/
// font-size) despite being the same button. Not a general-purpose
// variant-based Button — New track, icon buttons, tabs, menu items,
// etc. stay as their own thing; see prior discussion on why a single
// universal button wasn't the right call.
export function Button({ type = 'button', onClick, disabled, children }: ButtonProps) {
  return (
    <button type={type} className="btn-primary" onClick={onClick} disabled={disabled}>
      {children}
    </button>
  );
}
