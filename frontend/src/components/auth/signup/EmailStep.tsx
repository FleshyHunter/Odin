import type { FormEvent } from 'react';
import { Link } from 'react-router-dom';
import { Button } from '../../ui/Button/Button';
import '../authForm.css';

interface EmailStepProps {
  email: string;
  onEmailChange: (value: string) => void;
  onContinue: () => void;
}

// Step 1 of 3: collect just the email. No validation/checking — submit
// always advances to the verify step.
export function EmailStep({ email, onEmailChange, onContinue }: EmailStepProps) {
  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    onContinue();
  };

  return (
    <div className="signin-pane-content">
      <h1 className="headline display">Sign up</h1>
      <p className="subhead">Start learning on your terms.</p>

      <form onSubmit={handleSubmit}>
        <div className="field">
          <label htmlFor="signup-email">Email</label>
          <input
            type="email"
            id="signup-email"
            placeholder="you@example.com"
            autoComplete="email"
            value={email}
            onChange={(event) => onEmailChange(event.target.value)}
          />
        </div>
        <Button type="submit">Continue</Button>
      </form>

      <p className="switch-line">
        Already have an account? <Link className="link" to="/signin">Sign in</Link>
      </p>
    </div>
  );
}
