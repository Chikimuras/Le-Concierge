import { createI18n } from 'vue-i18n'

import en from './locales/en.json'
import fr from './locales/fr.json'

// Default locale is French per CLAUDE.md §9.8 (user-facing messages in FR).
// English is available for internal/dev UIs and future localisation.
type MessageSchema = typeof fr

export const SUPPORTED_LOCALES = ['fr', 'en'] as const
export type Locale = (typeof SUPPORTED_LOCALES)[number]

export const i18n = createI18n<[MessageSchema], Locale>({
  legacy: false,
  locale: 'fr',
  fallbackLocale: 'fr',
  messages: {
    fr,
    en,
  },
})
