import { useState } from 'react';
import { EmailStep } from './EmailStep';
import { VerifyStep } from './VerifyStep';

type OtpLoginStep = 'email' | 'verify';

interface OtpLoginFlowProps {
  onAuthenticated?: () => void;
}

// 2-step alternative to password sign-in (Auth section: "Login flow —
// TWO permanent, equally-valid methods, user's choice every time").
// Shorter than SignupFlow/ForgotPasswordFlow (no third step) since
// logging in needs no profile completion — a verified code logs the
// user straight in.
export function OtpLoginFlow({ onAuthenticated }: OtpLoginFlowProps) {
  const [step, setStep] = useState<OtpLoginStep>('email');
  const [email, setEmail] = useState('');

  if (step === 'email') {
    return <EmailStep email={email} onEmailChange={setEmail} onContinue={() => setStep('verify')} />;
  }

  return <VerifyStep email={email} onAuthenticated={onAuthenticated} />;
}
