import { Outlet } from 'react-router-dom';
import { Wordmark } from '../ui/WordMark/Wordmark';
import { BigDipperCanvas } from '../ui/BigDipper/BigDipperCanvas';
import './authLayout.css';

// Wraps both /signin and /signup as a React Router layout route (see
// App.tsx). This component — and therefore <BigDipperCanvas>, mounted
// once here — stays mounted across navigation between the two; only the
// <Outlet /> content (AuthForm) swaps. That's what keeps the constellation
// animation running continuously instead of resetting on every navigation
// between sign in and sign up.
export function AuthLayout() {
  return (
    <div className="auth-page">
      <nav>
        <Wordmark className="nav-wordmark" />
        <div className="nav-icon" tabIndex={0} role="button" aria-label="More options">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
            <circle cx="5" cy="12" r="1.8" />
            <circle cx="12" cy="12" r="1.8" />
            <circle cx="19" cy="12" r="1.8" />
          </svg>
        </div>
      </nav>

      <main className="split">
        <section className="signin-pane">
          <Outlet />
        </section>

        <BigDipperCanvas />
      </main>
    </div>
  );
}
