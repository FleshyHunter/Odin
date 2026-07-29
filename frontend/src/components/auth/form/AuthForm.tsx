import { useState, type FormEvent } from 'react';
import { Link } from 'react-router-dom';
import { useAuth } from '../../../hooks/useAuth';
import { Button } from '../../ui/Button/Button';
import { AuthNotice } from '../AuthNotice';
import './authForm.css';

interface AuthFormProps {
  onAuthenticated?: () => void;
}

// Sign-in only now — sign-up became its own multi-step wizard
// (components/auth/signup/SignupFlow.tsx: email -> verification code ->
// complete profile), since the two no longer share a single-screen shape.
export function AuthForm({ onAuthenticated }: AuthFormProps) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const { isLoading, error, signIn } = useAuth();

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await signIn(email, password);
      onAuthenticated?.();
    } catch {
      // error already surfaced via useAuth's error state below
    }
  };

  return (
    <div className="signin-pane-content">
      <h1 className="headline display">Sign in</h1>
      <p className="subhead">Pick up where you left off.</p>

      <form onSubmit={handleSubmit}>
        <div className="field">
          <label htmlFor="email">Email</label>
          <input
            type="email"
            id="email"
            placeholder="you@example.com"
            autoComplete="email"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
          />
        </div>
        <div className="field">
          <label htmlFor="password">Password</label>
          <input
            type="password"
            id="password"
            placeholder="••••••••"
            autoComplete="current-password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
        </div>
        <div className="row-between">
          <Link className="forgot-link" to="/forgot-password">
            Forgot password?
          </Link>
        </div>
        <Button type="submit" disabled={isLoading}>
          {isLoading ? 'Please wait…' : 'Sign in'}
        </Button>
        {error && <AuthNotice message={error} />}
      </form>

      <p className="switch-line">
        Don't have an account? <Link className="link" to="/signup">Sign up</Link>
      </p>
    </div>
  );
}
