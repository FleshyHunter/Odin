import { useState, type FormEvent } from 'react';
import { useAuth } from '../../../hooks/useAuth';
import { Button } from '../../ui/Button/Button';
import { AuthNotice } from '../AuthNotice';
import '../form/authForm.css';
import '../authWizard.css';

interface VerifyStepProps {
  email: string;
  onVerified: (displayName: string) => void;
}

// Step 2 of 3: verify the code from step 1
// (POST /auth/password-reset/verify-otp) — the response includes the
// account's display name, passed up to the final step for a "Hi <name>"
// prefill (safe to reveal only now: a matching code proves this is a
// real account, unlike the anti-enumeration-guarded request step).
export function VerifyStep({ email, onVerified }: VerifyStepProps) {
  const [code, setCode] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { verifyPasswordResetOtp, requestPasswordResetOtp } = useAuth();

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setIsLoading(true);
    setError(null);
    try {
      const displayName = await verifyPasswordResetOtp(email, code);
      onVerified(displayName);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Invalid or expired code');
    } finally {
      setIsLoading(false);
    }
  };

  const handleResend = async () => {
    try {
      await requestPasswordResetOtp(email);
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
          <label htmlFor="reset-code">Code</label>
          <input
            type="text"
            id="reset-code"
            placeholder="123456"
            required
            value={code}
            onChange={(event) => setCode(event.target.value)}
          />
        </div>
        <Button type="submit" disabled={isLoading}>
          {isLoading ? 'Please wait…' : 'Continue'}
        </Button>
        {error && <AuthNotice message={error} />}
      </form>

      <button type="button" className="resend-link" onClick={handleResend} disabled={isLoading}>
        Resend email
      </button>
    </div>
  );
}
