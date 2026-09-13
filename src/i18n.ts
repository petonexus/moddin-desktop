import { createI18n } from 'vue-i18n'
import { isLocale, localeFromBrowserLanguage, localeOptions, type Locale } from './i18n/locale'
import ptBR from './i18n/locales/pt-BR'
import en from './i18n/locales/en'
import es from './i18n/locales/es'
import { readLocalValue } from './services/storage'

export { localeOptions }
export type { Locale }

type TranslationMessages = Record<keyof typeof ptBR, string>

const messages = {
  'pt-BR': ptBR,
  en: en satisfies TranslationMessages,
  es: es satisfies TranslationMessages,
} satisfies Record<Locale, TranslationMessages>

export function getInitialLocale(): Locale {
  const saved = readLocalValue('moddin-locale')
  if (isLocale(saved)) return saved

  const browserLanguage = typeof navigator !== 'undefined' ? navigator.language : ''
  return localeFromBrowserLanguage(browserLanguage)
}

export const i18n = createI18n({
  legacy: false,
  locale: getInitialLocale(),
  fallbackLocale: 'pt-BR',
  messages,
})
