import { useState, type FormEvent } from 'react';
import { Button } from '../../ui/Button/Button';
import '../form/authForm.css';
import './signupFlow.css';

interface VerifyStepProps {
  email: string;
  onContinue: () => void;
  onContinueWithPassword: () => void;
}

// Step 2 of 3: enter the code "sent" to the step-1 email. No real code is
// ever sent or checked — Continue (and Resend, and Continue with
// password) are all pure navigation, per instruction.
export function VerifyStep({ email, onContinue, onContinueWithPassword }: VerifyStepProps) {
  const [code, setCode] = useState('');

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    onContinue();
  };

  return (
    <div className="signin-pane-content">
      <h1 className="headline display">Check your inbox</h1>
      <p className="subhead">Enter the verification code we just sent to {email || 'your email'}.</p>

      <form onSubmit={handleSubmit}>
        <div className="field">
          <label htmlFor="signup-code">Code</label>
          <input
            type="text"
            id="signup-code"
            placeholder="123456"
            value={code}
            onChange={(event) => setCode(event.target.value)}
          />
        </div>
        <Button type="submit">Continue</Button>
      </form>

      <button type="button" className="resend-link">
        Resend email
      </button>

      <div className="auth-divider">
        <span>OR</span>
      </div>

      <button type="button" className="secondary-btn" onClick={onContinueWithPassword}>
        Continue with password
      </button>
    </div>
  );
}
