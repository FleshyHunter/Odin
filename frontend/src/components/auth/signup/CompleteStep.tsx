import { useState, type FormEvent } from 'react';
import { useAuth } from '../../../hooks/useAuth';
import { Button } from '../../ui/Button/Button';
import { AuthNotice } from '../AuthNotice';
import '../form/authForm.css';
import '../authWizard.css';

interface CompleteStepProps {
  email: string;
  onComplete: () => void;
}

// Step 3 of 3: username + email (prefilled read-only from step 1) +
// password -> POST /auth/signup/complete. Only succeeds if step 2
// actually verified an OTP for this email recently (Auth section).
export function CompleteStep({ email, onComplete }: CompleteStepProps) {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { completeSignup } = useAuth();

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setIsLoading(true);
    setError(null);
    try {
      await completeSignup(email, username, password);
      onComplete();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Sign up failed');
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="signin-pane-content">
      <h1 className="headline display">Complete your profile</h1>
      <p className="subhead">Just a few more details.</p>

      <form onSubmit={handleSubmit}>
        <div className="field">
          <label htmlFor="signup-username">Username</label>
          <input
            type="text"
            id="signup-username"
            placeholder="yourname"
            autoComplete="username"
            required
            value={username}
            onChange={(event) => setUsername(event.target.value)}
          />
        </div>
        <div className="field">
          <label htmlFor="signup-complete-email">Email</label>
          <input type="email" id="signup-complete-email" value={email} readOnly />
        </div>
        <div className="field">
          <label htmlFor="signup-password">Password</label>
          <input
            type="password"
            id="signup-password"
            placeholder="••••••••"
            autoComplete="new-password"
            required
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
        </div>
        <Button type="submit" disabled={isLoading}>
          {isLoading ? 'Please wait…' : 'Continue'}
        </Button>
        {error && <AuthNotice message={error} />}
      </form>
    </div>
  );
}
