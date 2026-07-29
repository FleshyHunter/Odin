import { useState, type FormEvent } from 'react';
import { useAuth } from '../../../hooks/useAuth';
import { Button } from '../../ui/Button/Button';
import { AuthNotice } from '../AuthNotice';
import '../form/authForm.css';
import '../authWizard.css';

interface NewPasswordStepProps {
  email: string;
  displayName: string;
  onComplete: () => void;
}

// Step 3 of 3: username + email (both prefilled read-only — proven via
// step 2's OTP match, same pattern as signup's CompleteStep) + new
// password + confirm. Mismatch is checked client-side before ever
// calling the backend; a real backend error (e.g. password too short)
// surfaces the same way. POST /auth/password-reset/complete auto-logs
// in on success.
export function NewPasswordStep({ email, displayName, onComplete }: NewPasswordStepProps) {
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [mismatchError, setMismatchError] = useState<string | null>(null);
  const { isLoading, error, completePasswordReset } = useAuth();

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setMismatchError(null);
    if (password !== confirmPassword) {
      setMismatchError("Passwords don't match.");
      return;
    }
    try {
      await completePasswordReset(email, password);
      onComplete();
    } catch {
      // error already surfaced via useAuth's error state below
    }
  };

  const notice = mismatchError ?? error;

  return (
    <div className="signin-pane-content">
      <h1 className="headline display">Set a new password</h1>
      <p className="subhead">Hi {displayName}, choose a new password for your account.</p>

      <form onSubmit={handleSubmit}>
        <div className="field">
          <label htmlFor="reset-username">Username</label>
          <input type="text" id="reset-username" value={displayName} readOnly />
        </div>
        <div className="field">
          <label htmlFor="reset-email-display">Email</label>
          <input type="email" id="reset-email-display" value={email} readOnly />
        </div>
        <div className="field">
          <label htmlFor="reset-password">New password</label>
          <input
            type="password"
            id="reset-password"
            placeholder="••••••••"
            autoComplete="new-password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
        </div>
        <div className="field">
          <label htmlFor="reset-confirm-password">Confirm password</label>
          <input
            type="password"
            id="reset-confirm-password"
            placeholder="••••••••"
            autoComplete="new-password"
            value={confirmPassword}
            onChange={(event) => setConfirmPassword(event.target.value)}
          />
        </div>
        <Button type="submit" disabled={isLoading}>
          {isLoading ? 'Please wait…' : 'Continue'}
        </Button>
        {notice && <AuthNotice message={notice} />}
      </form>
    </div>
  );
}
