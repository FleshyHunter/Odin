import { useState } from 'react';
import { EmailStep } from './EmailStep';
import { VerifyStep } from './VerifyStep';
import { NewPasswordStep } from './NewPasswordStep';

type ForgotPasswordStep = 'email' | 'verify' | 'newPassword';

interface ForgotPasswordFlowProps {
  onAuthenticated?: () => void;
}

// Mirrors SignupFlow's shape (email -> verify code -> final step), same
// visual pattern, but ending in a new-password step instead of profile
// completion, and auto-logging in on success (backend/src/auth/
// handlers.rs's password_reset_complete — the user already proved
// mailbox ownership via OTP and just set fresh credentials, so a third
// login step would be pure friction, not extra security).
export function ForgotPasswordFlow({ onAuthenticated }: ForgotPasswordFlowProps) {
  const [step, setStep] = useState<ForgotPasswordStep>('email');
  const [email, setEmail] = useState('');
  const [displayName, setDisplayName] = useState('');

  if (step === 'email') {
    return <EmailStep email={email} onEmailChange={setEmail} onContinue={() => setStep('verify')} />;
  }

  if (step === 'verify') {
    return (
      <VerifyStep
        email={email}
        onVerified={(name) => {
          setDisplayName(name);
          setStep('newPassword');
        }}
      />
    );
  }

  return <NewPasswordStep email={email} displayName={displayName} onComplete={() => onAuthenticated?.()} />;
}
