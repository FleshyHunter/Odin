import { useState, type FormEvent } from 'react';
import { useAuth } from '../../../hooks/useAuth';
import { Button } from '../../ui/Button/Button';
import '../authForm.css';
import './signupFlow.css';

interface CompleteStepProps {
  email: string;
  onComplete: () => void;
}

// Step 3 of 3: username + email (prefilled read-only from step 1) +
// password. Still routes through the existing mock useAuth().signUp for
// consistency with how AuthForm completes sign-in — that call is already
// a stub (see api/auth.ts), so this doesn't add any new real checking.
export function CompleteStep({ email, onComplete }: CompleteStepProps) {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const { isLoading, signUp } = useAuth();

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    await signUp(email, password);
    onComplete();
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
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
        </div>
        <Button type="submit" disabled={isLoading}>
          {isLoading ? 'Please wait…' : 'Continue'}
        </Button>
      </form>
    </div>
  );
}
