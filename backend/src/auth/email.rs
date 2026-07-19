// Email Provider is locked as Resend (ARCHITECTURE.md: "simple API,
// good deliverability... swapping providers later is low-cost"), used
// for OTP codes and password reset links. Resend isn't set up yet
// (Mac-coding-first pass — Windows-side external services come later),
// so this is a trait — same swappable-behind-an-interface pattern
// already used for AcquisitionProvider — with a console-log stub
// implementation for now. Swapping in a real ResendEmailSender later
// means adding one new impl, not touching any caller in handlers.rs.

use async_trait::async_trait;

#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send_otp(&self, email: &str, code: &str);
    async fn send_password_reset(&self, email: &str, reset_link: &str);
}

pub struct ConsoleEmailSender;

#[async_trait]
impl EmailSender for ConsoleEmailSender {
    async fn send_otp(&self, email: &str, code: &str) {
        // Stands in for a real Resend call until RESEND_API_KEY is
        // configured — logged at info level so it's visible in normal
        // dev output, not hidden behind a debug filter.
        tracing::info!(%email, %code, "OTP email (stub — not actually sent, RESEND_API_KEY not set)");
    }

    async fn send_password_reset(&self, email: &str, reset_link: &str) {
        tracing::info!(%email, %reset_link, "Password reset email (stub — not actually sent, RESEND_API_KEY not set)");
    }
}
