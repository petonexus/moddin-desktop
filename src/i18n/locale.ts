export type Locale = 'pt-BR' | 'en' | 'es'

export const localeOptions: Array<{ value: Locale; label: string }> = [
  { value: 'pt-BR', label: 'Português (Brasil)' },
  { value: 'en', label: 'English' },
  { value: 'es', label: 'Español' },
]

export function isLocale(value: unknown): value is Locale {
  return value === 'pt-BR' || value === 'en' || value === 'es'
}

export function resolveLocale(value: string): Locale {
  return isLocale(value) ? value : 'en'
}

export function localeFromBrowserLanguage(value: string): Locale {
  const language = value.toLowerCase()
  if (language.startsWith('pt')) return 'pt-BR'
  if (language.startsWith('es')) return 'es'
  return 'en'
}

export function dateLocaleFor(value: string) {
  const locale = resolveLocale(value)
  if (locale === 'en') return 'en-US'
  if (locale === 'es') return 'es-ES'
  return 'pt-BR'
}
