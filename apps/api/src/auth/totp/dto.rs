//! Request / response DTOs for `/auth/2fa/*`.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ---- Start enrollment ------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EnrollStartResponse {
    /// `otpauth://totp/...` URL ready for QR rendering on the client. The
    /// secret is base32-encoded inside it — rendering it as a QR is the
    /// safe way to hand it over to the authenticator app.
    pub otpauth_url: String,

    /// Raw base32-encoded secret for manual authenticator entry (users
    /// without a camera on the device they authenticate from).
    pub secret_base32: String,
}

// ---- Confirm enrollment (finalise with first TOTP code) --------------------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EnrollVerifyRequest {
    /// Current 6-digit code shown by the user's authenticator.
    #[schema(example = "123456", min_length = 6, max_length = 6)]
    pub code: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EnrollVerifyResponse {
    /// The 10 single-use recovery codes, in `XXXX-XXXX` form. The API
    /// **never** returns these again — the caller must surface them to
    /// the user immediately and store them somewhere safe.
    pub recovery_codes: Vec<String>,
}

// ---- Step-up verify (post-login or step-up challenge) ----------------------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyRequest {
    /// Either a 6-digit TOTP code or a recovery code in `XXXX-XXXX` form
    /// (the dash is optional, case-insensitive).
    #[schema(example = "123456")]
    pub code: String,
}

/// Response body of a successful verify. `AuthenticatedResponse` carries
/// the fresh session (new SID + CSRF token after rotation); the extra
/// `used_recovery_code` flag lets the UI prompt the user to re-enroll.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VerifyResponse {
    #[serde(flatten)]
    pub session: crate::auth::dto::AuthenticatedResponse,
    pub used_recovery_code: bool,
}

// ---- Disable ---------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DisableRequest {
    /// Current TOTP code — the user must prove continued control of the
    /// authenticator before we let them remove it.
    #[schema(example = "123456", min_length = 6, max_length = 6)]
    pub code: String,
    /// Current account password. Re-verifying it defeats a stolen-session
    /// takeover of the disable endpoint.
    #[schema(format = "password")]
    pub password: String,
}
