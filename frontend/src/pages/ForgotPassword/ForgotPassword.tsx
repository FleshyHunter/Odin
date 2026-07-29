import { ForgotPasswordFlow } from '../../components/auth/forgot-password/ForgotPasswordFlow';

interface ForgotPasswordProps {
  onAuthenticated?: () => void;
}

// The nav + split layout + BigDipperCanvas shell lives in
// components/auth/layout/AuthLayout.tsx (a React Router layout route
// shared with Login/Signup) — kept mounted once so the constellation
// animation doesn't reset when navigating. ForgotPasswordFlow itself
// manages the 3-step wizard (email -> verify -> new password) internally.
export function ForgotPassword({ onAuthenticated }: ForgotPasswordProps) {
  return <ForgotPasswordFlow onAuthenticated={onAuthenticated} />;
}
