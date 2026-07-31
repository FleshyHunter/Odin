import { OtpLoginFlow } from '../../components/auth/otp-login/OtpLoginFlow';

interface OtpLoginProps {
  onAuthenticated?: () => void;
}

// Mirrors Login/Signup/ForgotPassword's thin-wrapper pattern — the nav +
// split layout + BigDipperCanvas shell lives in AuthLayout, shared
// across /signin, /signup, /forgot-password, and this route.
export function OtpLogin({ onAuthenticated }: OtpLoginProps) {
  return <OtpLoginFlow onAuthenticated={onAuthenticated} />;
}
