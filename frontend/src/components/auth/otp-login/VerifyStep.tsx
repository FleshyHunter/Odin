import { useState, type FormEvent } from 'react';
import { Link } from 'react-router-dom';
import { useAuth } from '../../../hooks/useAuth';
import { Button } from '../../ui/Button/Button';
import { AuthNotice } from '../AuthNotice';
import '../form/authForm.css';
import '../authWizard.css';

interface VerifyStepProps {
  email: string;
  onAuthenticated?: () => void;
}

// Step 2 of 2: verify the code from step 1 (POST /auth/login/verify-otp)
// — unlike signup/password-reset's verify-otp, this one already returns
// full tokens on success (login_verify_otp calls respond_with_tokens
// directly, backend/src/auth/handlers.rs), so a match logs the user
// straight in. No separate "complete" step: logging in needs no
// profile completion.
export function VerifyStep({ email, onAuthenticated }: VerifyStepProps) {
  const [code, setCode] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { verifyLoginOtp, requestLoginOtp } = useAuth();

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setIsLoading(true);
    setError(null);
    try {
      await verifyLoginOtp(email, code);
      onAuthenticated?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Invalid or expired code');
    } finally {
      setIsLoading(false);
    }
  };

  const handleResend = async () => {
    try {
      await requestLoginOtp(email);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not resend code');
    }
  };

  return (
    <div className="signin-pane-content">
      <h1 className="headline display">Check your inbox</h1>
      <p className="subhead">Enter the verification code we just sent to {email || 'your email'}.</p>

      <form onSubmit={handleSubmit}>
        <div className="field">
          <label htmlFor="otp-login-code">Code</label>
          <input
            type="text"
            id="otp-login-code"
            placeholder="123456"
            required
            value={code}
            onChange={(event) => setCode(event.target.value)}
          />
        </div>
        <Button type="submit" disabled={isLoading}>
          {isLoading ? 'Please wait…' : 'Log in'}
        </Button>
        {error && <AuthNotice message={error} />}
      </form>

      <button type="button" className="resend-link" onClick={handleResend} disabled={isLoading}>
        Resend email
      </button>

      <div className="auth-divider">
        <span>OR</span>
      </div>

      <Link className="secondary-btn" to="/signin">
        Continue with password
      </Link>
    </div>
  );
}
