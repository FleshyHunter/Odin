import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    // Pinned, not left to Vite's own auto-increment: a separate local
    // project ("veritas") already owns 5173, and letting Vite silently
    // fall back to whatever's free caused a real bug — the backend's
    // FRONTEND_ORIGIN CORS allowlist (backend/src/config.rs) only
    // matches one exact origin, so a silent port drift here breaks
    // credentialed auth requests (OTP, refresh cookie) with no obvious
    // error. strictPort: true makes that failure loud (Vite refuses to
    // start) instead of silently drifting to yet another port.
    port: 5174,
    strictPort: true,
    // Binds to all interfaces, not just loopback, so another device on
    // the same LAN (e.g. a phone) can reach this at the Mac's real LAN
    // IP:5174 instead of only localhost:5174. The backend's CORS
    // allow-list (FRONTEND_ORIGIN, backend/src/main.rs) has to separately
    // include that same LAN origin — this flag alone only makes the page
    // loadable, it doesn't touch CORS at all.
    host: true,
  },
})
