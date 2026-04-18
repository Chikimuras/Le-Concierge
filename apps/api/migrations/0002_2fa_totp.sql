-- TOTP 2FA enrollment + recovery codes.
--
-- Forward-only per CLAUDE.md §10. Row-level updates to `enrolled_at` and
-- `disabled_at` are expected; full wipes happen on disable.
--
-- References:
--   - CLAUDE.md §3.1 (2FA), §3.3 (encryption at rest)
--   - ADR 0002 — Security baseline
--   - ADR 0007 — TOTP 2FA

-- ---- user_totp ------------------------------------------------------------
-- 1:1 with `users`. `secret_cipher` is AES-256-GCM output laid out as
-- `nonce (12) || ciphertext (20) || tag (16)` = 48 bytes. The encryption
-- key lives in `APP_AUTH__TOTP_KEY` (env / Docker secret); the DB never
-- sees it. A row with `enrolled_at IS NULL` is a *pending* enrollment —
-- the authenticator paired but the first TOTP code has not been verified
-- yet. `disabled_at IS NOT NULL` means 2FA was explicitly removed; the
-- row is deleted in that case rather than flagged, so the `disabled_at`
-- column is here only as a future audit hook (see ADR 0007).

CREATE TABLE user_totp (
    user_id        uuid        PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    secret_cipher  bytea       NOT NULL
                               CHECK (octet_length(secret_cipher) = 48),
    enrolled_at    timestamptz,
    disabled_at    timestamptz,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);

CREATE TRIGGER user_totp_set_updated_at
    BEFORE UPDATE ON user_totp
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---- user_totp_recovery_codes --------------------------------------------
-- Generated once per successful enrollment (10 rows). Each `code_hash` is
-- a full Argon2id PHC string produced with the same pepper as passwords.
-- Consumption is a single `UPDATE ... SET used_at = now() WHERE id = $1
-- AND used_at IS NULL` inside the MFA-verify transaction so replay of the
-- same code in a concurrent request races on the row lock, not the app.

CREATE TABLE user_totp_recovery_codes (
    id         bigserial   PRIMARY KEY,
    user_id    uuid        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash  text        NOT NULL,
    used_at    timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- Verification iterates only unused rows for the caller — partial index
-- keeps that hot path tight even once many codes have been consumed
-- across the tenant base.
CREATE INDEX idx_user_totp_recovery_codes_unused
    ON user_totp_recovery_codes (user_id)
    WHERE used_at IS NULL;
