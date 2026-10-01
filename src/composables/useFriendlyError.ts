import { computed, type ComputedRef, type Ref } from 'vue'

export interface FriendlyErrorRule {
  /**
   * Matches the raw backend message. A rule without `match` is the
   * fallback and belongs last: it is what the user sees when nothing
   * more specific recognises the failure.
   */
  match?: RegExp
  /**
   * Restricts the rule to one action. Two failures can share a raw
   * string but need different advice, and the string alone cannot say
   * which one happened. Rules without a context apply everywhere.
   */
  context?: string
  title: string
  why: string
  /**
   * Whether the untranslated message stays reachable under a disclosure.
   * Useful for failures we recognise but cannot yet explain — hiding the
   * string would leave a power user with nothing to report back.
   */
  showRaw?: boolean
}

export interface FriendlyError {
  /** Empty when the message is already user-facing copy. */
  title: string
  why: string
  showRaw: boolean
  raw: string
}

export interface FriendlyErrorOptions {
  error: Ref<string | null>
  rules: () => FriendlyErrorRule[]
  /**
   * Which action produced the message, matched against `rule.context`.
   */
  context?: Ref<string | null>
  /**
   * Messages that are already translated copy — a missing consent, a game
   * that has to be selected first. They belong in the same callout, but
   * re-titling them as a failure would bury what they actually say.
   */
  verbatim?: Ref<readonly string[]>
}

export function useFriendlyError({
  error,
  rules,
  context,
  verbatim,
}: FriendlyErrorOptions): ComputedRef<FriendlyError | null> {
  return computed(() => {
    const raw = error.value?.trim()
    if (!raw) return null

    if (verbatim?.value.includes(raw)) {
      return { title: '', why: raw, showRaw: false, raw }
    }

    const current = context?.value ?? null
    const candidates = rules().filter((rule) => !rule.context || rule.context === current)
    const rule = candidates.find((candidate) => candidate.match?.test(raw))
      ?? candidates.find((candidate) => !candidate.match)
      ?? candidates[0]
    if (!rule) return { title: '', why: raw, showRaw: false, raw }

    return { title: rule.title, why: rule.why, showRaw: rule.showRaw ?? false, raw }
  })
}
