import { useMutation, useQuery, useQueryClient } from '@tanstack/vue-query'
import { type Ref, computed } from 'vue'
import { useRouter } from 'vue-router'

import { apiClient } from '@/lib/api-client'
import type {
  AuthenticatedResponse,
  Invite,
  InvitableRole,
  InviteListResponse,
  InvitePreview,
} from '@/lib/api-contracts'
import { useSessionStore } from '@/stores/session'

// ---- Manager-side ---------------------------------------------------------

export function useInvites(slug: Ref<string>) {
  return useQuery({
    queryKey: computed(() => ['invites', slug.value] as const),
    queryFn: () => apiClient.get(`orgs/${slug.value}/invites`).json<InviteListResponse>(),
    staleTime: 30_000,
  })
}

export interface CreateInvitePayload {
  email: string
  role: InvitableRole
}

export function useCreateInvite(slug: Ref<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (payload: CreateInvitePayload) =>
      apiClient.post(`orgs/${slug.value}/invites`, { json: payload }).json<Invite>(),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['invites', slug.value] })
    },
  })
}

export function useCancelInvite(slug: Ref<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      apiClient.delete(`orgs/${slug.value}/invites/${id}`).then(() => null),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['invites', slug.value] })
    },
  })
}

// ---- Invitee-side ---------------------------------------------------------

/** `POST /auth/invites/preview`. Not a `useQuery` because the token
 *  lives in the URL query string — we feed it lazily and the call
 *  itself is a non-idempotent POST per RFC 7807 conventions. */
export function useInvitePreview() {
  return useMutation({
    mutationFn: (token: string) =>
      apiClient.post('auth/invites/preview', { json: { token } }).json<InvitePreview>(),
  })
}

/** Authed accept: server checks email-match and links the membership. */
export function useAcceptInvite() {
  const session = useSessionStore()
  const router = useRouter()
  return useMutation({
    mutationFn: (token: string) =>
      apiClient.post('auth/invites/accept', { json: { token } }).json<AuthenticatedResponse>(),
    onSuccess: (data) => {
      session.setFromAuth(data)
      void router.replace({ name: 'dashboard' })
    },
  })
}

export interface SignupAndAcceptPayload {
  token: string
  password: string
}

/** Anon signup-and-accept: backend creates the user with the invite's
 *  email, links the membership, mints a session. We flow the fresh
 *  session into the store just like `useLogin`. */
export function useSignupAndAcceptInvite() {
  const session = useSessionStore()
  const router = useRouter()
  return useMutation({
    mutationFn: (payload: SignupAndAcceptPayload) =>
      apiClient.post('auth/invites/signup', { json: payload }).json<AuthenticatedResponse>(),
    onSuccess: (data) => {
      session.setFromAuth(data)
      void router.replace({ name: 'dashboard' })
    },
  })
}
