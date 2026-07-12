import { AuthForm } from '../../components/auth/AuthForm';

interface LoginProps {
  onAuthenticated?: () => void;
}

// The nav + split layout + BigDipperCanvas shell now lives in
// components/auth/AuthLayout.tsx (a React Router layout route wrapping
// both this and Signup) — kept mounted once so the constellation
// animation doesn't reset when navigating between /signin and /signup.
export function Login({ onAuthenticated }: LoginProps) {
  return <AuthForm onAuthenticated={onAuthenticated} />;
}
