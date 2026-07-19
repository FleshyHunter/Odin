import { SignupFlow } from '../../components/auth/signup/SignupFlow';

interface SignupProps {
  onAuthenticated?: () => void;
}

// The nav + split layout + BigDipperCanvas shell lives in
// components/auth/layout/AuthLayout.tsx (a React Router layout route wrapping
// both this and Login) — kept mounted once so the constellation
// animation doesn't reset when navigating. SignupFlow itself manages the
// 3-step wizard (email -> verify -> complete) internally.
export function Signup({ onAuthenticated }: SignupProps) {
  return <SignupFlow onAuthenticated={onAuthenticated} />;
}
