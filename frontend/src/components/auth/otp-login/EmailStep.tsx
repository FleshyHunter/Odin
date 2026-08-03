import { useState, type FormEvent } from 'react';
import { Link } from 'react-router-dom';
import { useAuth } from '../../../hooks/useAuth';
import { Button } from '../../ui/Button/Button';
import { AuthNotice } from '../AuthNotice';
import '../form/authForm.css';
import '../authWizard.css';

interface EmailStepProps {
  email: string;
  onEmailChange: (value: string) => void;
  onContinue: () => void;
}

// Step 1 of 2: collect the email and request a real login OTP
// (POST /auth/login/request-otp) — same anti-enumeration shape as
// signup/password-reset's request-otp (always 200), so "Continue"
// always advances once the call succeeds.
export function EmailStep({ email, onEmailChange, onContinue }: EmailStepProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { requestLoginOtp } = useAuth();

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setIsLoading(true);
    setError(null);
    try {
      await requestLoginOtp(email);
      onContinue();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not send verification code');
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="signin-pane-content">
      <h1 className="headline display">Log in with a code</h1>
      <p className="subhead">We'll email you a one-time code.</p>

      <form onSubmit={handleSubmit}>
        <div className="field">
          <label htmlFor="otp-login-email">Email</label>
          <input
            type="email"
            id="otp-login-email"
            placeholder="you@example.com"
            autoComplete="email"
            required
            value={email}
            onChange={(event) => onEmailChange(event.target.value)}
          />
        </div>
        <Button type="submit" disabled={isLoading}>
          {isLoading ? 'Please wait…' : 'Continue'}
        </Button>
        {error && <AuthNotice message={error} />}
      </form>

      <div className="auth-divider">
        <span>OR</span>
      </div>

      <Link className="secondary-btn" to="/signin">
        Continue with password
      </Link>

      <p className="switch-line">
        Don't have an account? <Link className="link" to="/signup">Sign up</Link>
      </p>
    </div>
  );
}
