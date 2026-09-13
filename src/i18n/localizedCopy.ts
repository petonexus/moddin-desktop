import { resolveLocale, type Locale } from './locale'

type StringCopy = Record<string, string>

type MatchingCopy<Base extends StringCopy> = Record<keyof Base, string>

export function defineLocalizedCopy<const Base extends StringCopy>(
  ptBR: Base,
  en: MatchingCopy<Base>,
  es: MatchingCopy<Base>,
) {
  return {
    'pt-BR': ptBR,
    en,
    es,
  } satisfies Record<Locale, MatchingCopy<Base>>
}

export function localizedCopyFor<T>(messages: Record<Locale, T>, locale: string): T {
  return messages[resolveLocale(locale)]
}
