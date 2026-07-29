-- Password reset moved from a link+token mechanism (users.password_
-- reset_token_hash/_expires_at/_used_at) to the same Redis-staged OTP
-- mechanism already used by signup/login (see auth/otp.rs) — these
-- columns are now fully dead, same reasoning as the OTP-first signup
-- design never needing OTP columns on this table at all.
ALTER TABLE users
    DROP COLUMN password_reset_token_hash,
    DROP COLUMN password_reset_expires_at,
    DROP COLUMN password_reset_used_at;
