import { resolveLocale, type Locale } from './locale'

type StringCopy = Readonly<Record<string, string>>

type MatchingCopy<Base extends StringCopy> = {
  readonly [Key in keyof Base]: string
}

type LocalizedCopy<Base extends StringCopy> = Record<Locale, MatchingCopy<Base>>

export function defineLocalizedCopy<const Base extends StringCopy>(
  ptBR: Base,
  en: MatchingCopy<Base>,
  es: MatchingCopy<Base>,
): LocalizedCopy<Base> {
  return {
    'pt-BR': ptBR,
    en,
    es,
  }
}

export function localizedCopyFor<T>(messages: Record<Locale, T>, locale: string): T {
  return messages[resolveLocale(locale)]
}
