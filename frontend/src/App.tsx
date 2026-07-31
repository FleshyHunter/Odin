import { Navigate, Route, Routes, useNavigate } from 'react-router-dom';
import { AuthLayout } from './components/auth/layout/AuthLayout';
import { Login } from './pages/Login/Login';
import { Signup } from './pages/Signup/Signup';
import { ForgotPassword } from './pages/ForgotPassword/ForgotPassword';
import { OtpLogin } from './pages/OtpLogin/OtpLogin';
import { SessionLayout } from './pages/Session/SessionLayout';
import { ChatView } from './pages/Session/ChatView';
import { Projects } from './pages/Projects/Projects';
import { Tracks } from './pages/Tracks/Tracks';
import { useAuth } from './hooks/useAuth';

// isAuthenticated is now derived straight from the shared AuthContext's
// session (see hooks/useAuth.tsx) rather than its own local boolean —
// every sign-in/signup/reset step already updates that same shared
// session on success, so this just reflects it instead of needing its
// own copy kept in sync via callback params. /chat is the landing/
// chooser route (post-login) and also the unified memoryless-or-track-
// backed conversation route (/chat/:id, not yet added); /projects and
// /tracks are each a browse-all index (plural, a deliberate deviation
// from ARCHITECTURE_LOCK.md's singular /project convention). All three
// are real, linkable/refreshable URLs rendered under one SessionLayout
// so the sidebar never remounts when switching between them (same
// pattern as AuthLayout for /signin<->/signup).
function App() {
  const { session, isBootstrapping, signOut } = useAuth();
  const isAuthenticated = session !== null;
  const navigate = useNavigate();

  const handleAuthenticated = () => {
    navigate('/chat');
  };

  const handleSignOut = async () => {
    await signOut();
    navigate('/signin');
  };

  // Avoids flashing the sign-in page for the brief moment the silent
  // refresh-cookie check (hooks/useAuth.tsx's bootstrap effect) is
  // still in flight on first load.
  if (isBootstrapping) {
    return null;
  }

  return (
    <Routes>
      {/* AuthLayout is a layout route — it (and the BigDipperCanvas it
          renders) stays mounted across /signin <-> /signup navigation;
          only the matched child below swaps via <Outlet />. */}
      <Route element={<AuthLayout />}>
        <Route
          path="/signin"
          element={isAuthenticated ? <Navigate to="/chat" replace /> : <Login onAuthenticated={handleAuthenticated} />}
        />
        <Route
          path="/signup"
          element={
            isAuthenticated ? <Navigate to="/chat" replace /> : <Signup onAuthenticated={handleAuthenticated} />
          }
        />
        <Route
          path="/forgot-password"
          element={
            isAuthenticated ? (
              <Navigate to="/chat" replace />
            ) : (
              <ForgotPassword onAuthenticated={handleAuthenticated} />
            )
          }
        />
        <Route
          path="/signin/otp"
          element={
            isAuthenticated ? <Navigate to="/chat" replace /> : <OtpLogin onAuthenticated={handleAuthenticated} />
          }
        />
      </Route>
      <Route
        element={
          isAuthenticated && session ? (
            <SessionLayout
              profileName={session.user.displayName}
              email={session.user.email}
              onSignOut={handleSignOut}
            />
          ) : (
            <Navigate to="/signin" replace />
          )
        }
      >
        <Route path="/chat" element={<ChatView />} />
        <Route path="/projects" element={<Projects />} />
        <Route path="/tracks" element={<Tracks />} />
      </Route>
      <Route path="*" element={<Navigate to="/chat" replace />} />
    </Routes>
  );
}

export default App;
