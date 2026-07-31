import { useState } from 'react';
import { EmailStep } from './EmailStep';
import { VerifyStep } from './VerifyStep';
import { CompleteStep } from './CompleteStep';

type SignupStep = 'email' | 'verify' | 'complete';

interface SignupFlowProps {
  onAuthenticated?: () => void;
}

// Multi-step signup wizard: email -> verification code -> complete profile.
// Internal step-state within the single /signup route, not separate
// routes per step — this flow is only ever reached via in-app navigation
// (never an external link like a real "click to verify" email would be),
// so there's no need for each step to be independently linkable, and
// internal state avoids threading the in-progress email through URL params.
//
// Each step now calls the real backend (POST /auth/signup/request-otp,
// verify-otp, complete) — "Continue" only advances once that step's
// call actually succeeds.
export function SignupFlow({ onAuthenticated }: SignupFlowProps) {
  const [step, setStep] = useState<SignupStep>('email');
  const [email, setEmail] = useState('');

  if (step === 'email') {
    return <EmailStep email={email} onEmailChange={setEmail} onContinue={() => setStep('verify')} />;
  }

  if (step === 'verify') {
    return (
      <VerifyStep
        email={email}
        onContinue={() => setStep('complete')}
        onContinueWithPassword={() => setStep('complete')}
      />
    );
  }

  return <CompleteStep email={email} onComplete={() => onAuthenticated?.()} />;
}
