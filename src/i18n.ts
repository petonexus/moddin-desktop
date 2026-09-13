import { createI18n } from 'vue-i18n'
import ptBR from './i18n/locales/pt-BR'
import en from './i18n/locales/en'
import es from './i18n/locales/es'

export type Locale = 'pt-BR' | 'en' | 'es'

type TranslationMessages = Record<keyof typeof ptBR, string>

export const localeOptions: Array<{ value: Locale; label: string }> = [
  { value: 'pt-BR', label: 'Português (Brasil)' },
  { value: 'en', label: 'English' },
  { value: 'es', label: 'Español' },
]

const messages = {
  'pt-BR': ptBR,
  en: en satisfies TranslationMessages,
  es: es satisfies TranslationMessages,
} satisfies Record<Locale, TranslationMessages>

export function getInitialLocale(): Locale {
  try {
    const saved = window.localStorage.getItem('moddin-locale')
    if (saved === 'pt-BR' || saved === 'en' || saved === 'es') return saved
  } catch {
    // Ignore unavailable storage, such as privacy-restricted browser contexts.
  }

  const browserLanguage = typeof navigator !== 'undefined' ? navigator.language.toLowerCase() : ''
  if (browserLanguage.startsWith('pt')) return 'pt-BR'
  if (browserLanguage.startsWith('es')) return 'es'
  return 'en'
}

export const i18n = createI18n({
  legacy: false,
  locale: getInitialLocale(),
  fallbackLocale: 'pt-BR',
  messages,
})
