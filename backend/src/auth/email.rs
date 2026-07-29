// Email Provider: real delivery via SmtpEmailSender, a deliberate swap
// from the originally-locked "Resend" provider (ARCHITECTURE.md) made
// when actually setting this up — plain SMTP through a real account
// (Gmail, in practice) needs no domain/DNS verification at all to send
// to arbitrary real recipients, unlike Resend's no-domain mode (which
// only sends to the account owner), since the sending domain
// (gmail.com) is already fully trusted infrastructure. Config is
// generic SMTP (relay/username/password), not Gmail-specific, so a
// future provider swap is just different env var values, not a code
// change. Falls back to ConsoleEmailSender when unset — same
// swappable-behind-an-interface pattern already used for
// AcquisitionProvider.

use async_trait::async_trait;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send_otp(&self, email: &str, code: &str);
    async fn send_password_reset(&self, email: &str, reset_link: &str);
}

pub struct ConsoleEmailSender;

#[async_trait]
impl EmailSender for ConsoleEmailSender {
    async fn send_otp(&self, email: &str, code: &str) {
        // Stands in for a real send until SMTP_* is configured —
        // logged at info level so it's visible in normal dev output,
        // not hidden behind a debug filter.
        tracing::info!(%email, %code, "OTP email (stub — not actually sent, SMTP not configured)");
    }

    async fn send_password_reset(&self, email: &str, reset_link: &str) {
        tracing::info!(%email, %reset_link, "Password reset email (stub — not actually sent, SMTP not configured)");
    }
}

pub struct SmtpEmailSender {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from_address: Mailbox,
}

impl SmtpEmailSender {
    /// Constructed once at startup (main.rs) — a failure here is a
    /// configuration problem (bad relay host, malformed username), so
    /// callers are expected to `.expect()` it, same "fail clearly and
    /// early" pattern as JWT_SECRET/DATABASE_URL.
    pub fn new(relay: &str, username: String, password: String) -> Result<Self, String> {
        let from_address: Mailbox = username
            .parse()
            .map_err(|err| format!("SMTP_USERNAME {username:?} is not a valid email address: {err}"))?;

        let creds = Credentials::new(username, password);
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(relay)
            .map_err(|err| format!("failed to configure SMTP relay {relay:?}: {err}"))?
            .credentials(creds)
            .build();

        Ok(Self { mailer, from_address })
    }

    async fn send(&self, to: &str, subject: &str, body: String) {
        let to_address: Mailbox = match to.parse() {
            Ok(addr) => addr,
            Err(err) => {
                tracing::error!(%to, ?err, "invalid recipient email address, not sending");
                return;
            }
        };

        let email = match Message::builder()
            .from(self.from_address.clone())
            .to(to_address)
            .subject(subject)
            .body(body)
        {
            Ok(email) => email,
            Err(err) => {
                tracing::error!(?err, "failed to build email message");
                return;
            }
        };

        if let Err(err) = self.mailer.send(email).await {
            tracing::error!(%to, ?err, "failed to send email via SMTP");
        }
    }
}

#[async_trait]
impl EmailSender for SmtpEmailSender {
    async fn send_otp(&self, email: &str, code: &str) {
        self.send(
            email,
            "Your Odin verification code",
            format!(
                "Your one-time code is: {code}\n\n\
                 This code expires shortly. If you didn't request this, you can ignore this email."
            ),
        )
        .await;
    }

    async fn send_password_reset(&self, email: &str, reset_link: &str) {
        self.send(
            email,
            "Reset your Odin password",
            format!(
                "Reset your password here: {reset_link}\n\n\
                 If you didn't request this, you can ignore this email."
            ),
        )
        .await;
    }
}
