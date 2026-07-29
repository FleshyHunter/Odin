import type { FormEvent } from 'react';
import { Link } from 'react-router-dom';
import { useAuth } from '../../../hooks/useAuth';
import { Button } from '../../ui/Button/Button';
import { AuthNotice } from '../AuthNotice';
import '../form/authForm.css';

interface EmailStepProps {
  email: string;
  onEmailChange: (value: string) => void;
  onContinue: () => void;
}

// Step 1 of 3: collect the email and request a real reset OTP
// (POST /auth/password-reset/request-otp) — this endpoint always
// returns 200 regardless of whether the account exists (anti-
// enumeration), so "Continue" always advances once the call succeeds.
export function EmailStep({ email, onEmailChange, onContinue }: EmailStepProps) {
  const { isLoading, error, requestPasswordResetOtp } = useAuth();

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await requestPasswordResetOtp(email);
      onContinue();
    } catch {
      // error already surfaced via useAuth's error state below
    }
  };

  return (
    <div className="signin-pane-content">
      <h1 className="headline display">Reset your password</h1>
      <p className="subhead">Enter your email and we'll send you a verification code.</p>

      <form onSubmit={handleSubmit}>
        <div className="field">
          <label htmlFor="reset-email">Email</label>
          <input
            type="email"
            id="reset-email"
            placeholder="you@example.com"
            autoComplete="email"
            value={email}
            onChange={(event) => onEmailChange(event.target.value)}
          />
        </div>
        <Button type="submit" disabled={isLoading}>
          {isLoading ? 'Please wait…' : 'Continue'}
        </Button>
        {error && <AuthNotice message={error} />}
      </form>

      <p className="switch-line">
        Remembered it? <Link className="link" to="/signin">Sign in</Link>
      </p>
    </div>
  );
}
