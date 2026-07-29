import './authNotice.css';

interface AuthNoticeProps {
  message: string;
}

// Shared error display for auth forms (sign-in, complete-signup,
// forgot-password) — same error-tone visual language as ComposerNotice,
// but standalone: no dismiss/action-button affordances, since a form
// error just needs to sit below its submit button until the next
// submit replaces or clears it.
export function AuthNotice({ message }: AuthNoticeProps) {
  return (
    <div className="auth-notice" role="alert">
      {message}
    </div>
  );
}
