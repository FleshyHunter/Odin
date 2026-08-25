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

use std::time::Duration;

use async_trait::async_trait;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

#[async_trait]
pub trait EmailSender: Send + Sync {
    // One method covers signup verification, standing OTP login, AND
    // password reset (see auth/otp.rs) — all three are "here's your
    // one-time code" emails, just staged under different Redis key
    // namespaces server-side. No separate reset-link email exists
    // anymore (see Auth section's password-reset note for why).
    async fn send_otp(&self, email: &str, code: &str);
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
}

pub struct SmtpEmailSender {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from_address: Mailbox,
    // Found live 2026-08-25 — see config.rs's own note on
    // smtp_send_timeout_seconds for the real hang this closes. Applied
    // around the whole send() call (connect + STARTTLS + auth + DATA),
    // not passed into lettre's own builder — lettre's SmtpTransportBuilder
    // has no single "whole operation" timeout knob of its own, only
    // lower-level connection options, so wrapping the call site is the
    // simpler, correct fix here.
    send_timeout: Duration,
}

impl SmtpEmailSender {
    /// Constructed once at startup (main.rs) — a failure here is a
    /// configuration problem (bad relay host, malformed username), so
    /// callers are expected to `.expect()` it, same "fail clearly and
    /// early" pattern as JWT_SECRET/DATABASE_URL.
    pub fn new(
        relay: &str,
        username: String,
        password: String,
        send_timeout_secs: u64,
    ) -> Result<Self, String> {
        let from_address: Mailbox = username
            .parse()
            .map_err(|err| format!("SMTP_USERNAME {username:?} is not a valid email address: {err}"))?;

        let creds = Credentials::new(username, password);
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(relay)
            .map_err(|err| format!("failed to configure SMTP relay {relay:?}: {err}"))?
            .credentials(creds)
            .build();

        Ok(Self {
            mailer,
            from_address,
            send_timeout: Duration::from_secs(send_timeout_secs),
        })
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

        match tokio::time::timeout(self.send_timeout, self.mailer.send(email)).await {
            Ok(Ok(_)) => {}
            Ok(Err(err)) => {
                tracing::error!(%to, ?err, "failed to send email via SMTP");
            }
            Err(_elapsed) => {
                tracing::error!(
                    %to,
                    timeout_secs = self.send_timeout.as_secs(),
                    "SMTP send timed out — the connection likely stalled mid-handshake \
                     (seen on networks that allow the initial TCP connect but then \
                     silently drop/throttle the actual STARTTLS traffic, e.g. some \
                     cellular hotspots), not a DNS/connection-refused failure"
                );
            }
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
}
