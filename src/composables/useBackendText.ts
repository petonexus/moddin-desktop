import { useI18n } from 'vue-i18n'
import type { BackendTextKey, LocalizedText } from '../i18n/backendIds'

/**
 * Renders a value that has been through the UX-21 boundary.
 *
 * The component asks for a sentence; it never sees the backend id, and
 * it never has to know whether this particular string was a declared id.
 * A value with no `key` renders its own `text` — the documented
 * fallback in `src/i18n/backendIds.ts`, which exists because a card
 * with no description and a log entry with no action are both worse
 * than a card in English.
 */
export function useBackendText() {
  const { t } = useI18n()

  function text(value: LocalizedText | null | undefined): string {
    if (!value) return ''
    return value.key ? t(value.key, value.params ?? {}) : value.text
  }

  /**
   * The same rendering for a field that carries its key beside the
   * original string — the capability summary's `description` and a check
   * outcome's `label`, whose wire types are shared with callers outside
   * this feature and could not carry a `LocalizedText` without breaking
   * them.
   */
  function keyed(key: BackendTextKey | undefined, fallback: string): string {
    return key ? t(key) : fallback
  }

  return { text, keyed }
}
