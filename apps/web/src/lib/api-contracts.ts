/**
 * TypeScript types and Zod schemas that mirror the backend DTOs under
 * `apps/api/src/auth/dto.rs` and `apps/api/src/session/dto.rs`. Acts as a
 * single source of truth for the web app so validation rules (regex,
 * bounds) cannot drift from the server.
 *
 * Will be replaced by generated bindings from `packages/contracts/` once
 * `openapi-typescript` is wired to the live `/openapi.json`.
 */
import { z } from 'zod'

export const PASSWORD_MIN = 12
export const ORG_NAME_MAX = 200
export const ORG_SLUG_MIN = 2
export const ORG_SLUG_MAX = 64

// Mirrors `Slug::parse` in `apps/api/src/auth/domain.rs` — drift here
// means the client accepts input the server rejects (or vice-versa).
export const ORG_SLUG_REGEX = /^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$/

export const roleSchema = z.enum(['owner', 'manager', 'cleaner', 'guest', 'admin'])
export type Role = z.infer<typeof roleSchema>

export interface MembershipSummary {
  org_id: string
  org_slug: string
  org_name: string
  role: Role
}

export interface SessionMeta {
  user_id: string
  csrf_token: string
  mfa_verified: boolean
  created_at: string
  absolute_expires_at: string
}

export interface AuthenticatedResponse {
  session: SessionMeta
  user_id: string
  memberships: MembershipSummary[]
  is_platform_admin: boolean
  /** True when the user has an active 2FA enrollment. */
  mfa_enrolled: boolean
  /** True when the user's role makes 2FA mandatory (admin/manager). */
  mfa_required: boolean
}

export interface MeResponse extends AuthenticatedResponse {
  resolved_at: string
}

export interface SignupRequest {
  email: string
  password: string
  organization_slug: string
  organization_name: string
}

export interface LoginRequest {
  email: string
  password: string
}

export const problemKindSchema = z.enum([
  'not_found',
  'unauthorized',
  'forbidden',
  'validation',
  'conflict',
  'rate_limited',
  'unavailable',
  'internal',
])
export type ProblemKind = z.infer<typeof problemKindSchema>

/**
 * RFC 7807 `application/problem+json` body with our `kind` discriminant
 * and optional trace id. Always validated before use — a 4xx/5xx from a
 * misbehaving proxy could otherwise yield `undefined.kind`.
 */
export const problemDetailsSchema = z.object({
  type: z.string(),
  title: z.string(),
  status: z.number(),
  detail: z.string(),
  kind: problemKindSchema,
  trace_id: z.string().optional(),
})
export type ProblemDetails = z.infer<typeof problemDetailsSchema>

/**
 * Builder so views can parameterise error messages with i18n lookups.
 * Returned schema is disposable — locale changes do not re-translate
 * messages on an already-parsed form.
 */
export function loginRequestSchema(messages: {
  emailRequired: string
  emailInvalid: string
  passwordRequired: string
}) {
  return z.object({
    email: z.string().min(1, messages.emailRequired).email(messages.emailInvalid),
    password: z.string().min(1, messages.passwordRequired),
  })
}

export function signupRequestSchema(messages: {
  emailRequired: string
  emailInvalid: string
  passwordMin: string
  orgNameRequired: string
  orgNameTooLong: string
  slugInvalid: string
}) {
  return z.object({
    email: z.string().min(1, messages.emailRequired).email(messages.emailInvalid),
    password: z.string().min(PASSWORD_MIN, messages.passwordMin),
    organization_name: z
      .string()
      .min(1, messages.orgNameRequired)
      .max(ORG_NAME_MAX, messages.orgNameTooLong),
    organization_slug: z
      .string()
      .min(ORG_SLUG_MIN)
      .max(ORG_SLUG_MAX)
      .regex(ORG_SLUG_REGEX, messages.slugInvalid),
  })
}

// ---- TOTP 2FA (Phase 4c) --------------------------------------------------

/** Length of a raw TOTP code (RFC 6238 default) — 6 decimal digits. */
export const TOTP_CODE_LEN = 6
/** Recovery codes: 8 chars, sampled from a confusable-free alphabet. */
export const RECOVERY_CODE_LEN = 8

/** Accepts a 6-digit TOTP code or an 8-char recovery code (dash optional,
 *  case-insensitive). Used for the step-up flow; enrollment verify only
 *  accepts TOTP. */
export function totpVerifyRequestSchema(messages: { codeInvalid: string }) {
  return z.object({
    code: z
      .string()
      .transform((v) => v.replace(/[\s-]/g, '').toUpperCase())
      .pipe(z.string().regex(/^(\d{6}|[A-Z0-9]{8})$/, messages.codeInvalid)),
  })
}

/** Strict 6-digit TOTP for `POST /auth/2fa/enroll/verify`. */
export function totpEnrollVerifySchema(messages: { codeInvalid: string }) {
  return z.object({
    code: z.string().regex(/^\d{6}$/, messages.codeInvalid),
  })
}

/** Password + current TOTP. Password is just `min(1)` — the server
 *  verifies the real password hash, the client just forwards. */
export function disableRequestSchema(messages: { passwordRequired: string; codeInvalid: string }) {
  return z.object({
    password: z.string().min(1, messages.passwordRequired),
    code: z.string().regex(/^\d{6}$/, messages.codeInvalid),
  })
}

export interface EnrollStartResponse {
  /** `otpauth://totp/...` URL ready for QR rendering. */
  otpauth_url: string
  /** Base32-encoded secret for manual authenticator entry. */
  secret_base32: string
}

export interface EnrollVerifyResponse {
  /** 10 single-use recovery codes in `XXXX-XXXX` form. Server never
   *  returns them again — surface to the user immediately. */
  recovery_codes: string[]
}

/** `POST /auth/2fa/verify` body: flattened AuthenticatedResponse plus a
 *  flag set when the caller consumed a recovery code (UI warns them). */
export interface TotpVerifyResponse extends AuthenticatedResponse {
  used_recovery_code: boolean
}
