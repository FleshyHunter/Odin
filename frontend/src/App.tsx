import { useState } from 'react';
import { Navigate, Route, Routes, useNavigate } from 'react-router-dom';
import { AuthLayout } from './components/auth/layout/AuthLayout';
import { Login } from './pages/Login/Login';
import { Signup } from './pages/Signup/Signup';
import { SessionLayout } from './pages/Session/SessionLayout';
import { ChatView } from './pages/Session/ChatView';
import { Projects } from './pages/Projects/Projects';
import { Tracks } from './pages/Tracks/Tracks';

// No real backend session yet — isAuthenticated is still just local state
// (see Login/useAuth's mock signIn/signUp). /chat is the landing/chooser
// route (post-login) and also the unified memoryless-or-track-backed
// conversation route (/chat/:id, not yet added); /projects and /tracks are
// each a browse-all index (plural, a deliberate deviation from
// ARCHITECTURE_LOCK.md's singular /project convention). All three are
// real, linkable/refreshable URLs rendered under one SessionLayout so the
// sidebar never remounts when switching between them (same pattern as
// AuthLayout for /signin<->/signup).
function App() {
  const [isAuthenticated, setIsAuthenticated] = useState(false);
  const navigate = useNavigate();

  const handleAuthenticated = () => {
    setIsAuthenticated(true);
    navigate('/chat');
  };

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
      </Route>
      <Route
        element={
          isAuthenticated ? <SessionLayout profileName="Profile 1" /> : <Navigate to="/signin" replace />
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
