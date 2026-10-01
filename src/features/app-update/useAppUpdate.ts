import { computed, ref } from 'vue'
import { readLocalValue, removeLocalValue, writeLocalValue } from '../../services/storage'
import {
  checkForAppUpdate,
  installAppUpdate,
  onAppUpdateProgress,
  readAppUpdateStatus,
} from './service'
import type { AppUpdateStatus } from './types'

/**
 * Which action produced `error`. Two failures can share a raw string and
 * still need different advice, and the string alone cannot say which.
 */
export type AppUpdateAction = 'check' | 'install'

/** The panel is in exactly one of these. */
export type AppUpdatePhase =
  | 'idle'
  | 'checking'
  | 'up-to-date'
  | 'available'
  | 'declined'
  | 'unavailable'
  | 'not-configured'
  | 'installing'
  | 'finishing'

const DECLINED_STORAGE_KEY = 'moddin-update-declined'

/**
 * The version the user last turned down.
 *
 * Stored rather than held in memory because the rule has to survive the
 * restart that a declined update avoids: there is no timer in this
 * feature, so nothing would re-ask anyway, and a stored value is what
 * makes the backend's refusal auditable.
 */
export function readDeclinedVersion() {
  return readLocalValue(DECLINED_STORAGE_KEY)
}

function rememberDeclined(version: string) {
  writeLocalValue(DECLINED_STORAGE_KEY, version)
}

function forgetDeclined() {
  removeLocalValue(DECLINED_STORAGE_KEY)
}

/**
 * The updater's state, and the three commands the panel can issue.
 *
 * Opt-in by construction: nothing here runs until `check()` is called,
 * and `install()` only accepts the version the last `check()` offered,
 * so a confirmation can never install something the user did not read.
 */
export function useAppUpdate() {
  const phase = ref<AppUpdatePhase>('idle')
  const error = ref<string | null>(null)
  const errorContext = ref<AppUpdateAction | null>(null)

  const status = ref<AppUpdateStatus | null>(null)
  const offeredVersion = ref<string | null>(null)
  const notes = ref<string | null>(null)
  const publishedAt = ref<string | null>(null)
  const detail = ref<string | null>(null)
  const declinedVersion = ref<string | null>(null)

  const progress = ref<{ downloaded: number; total: number | null } | null>(null)
  const installed = ref<string | null>(null)

  function fail(action: AppUpdateAction, err: unknown) {
    error.value = err instanceof Error ? err.message : String(err)
    errorContext.value = action
  }

  function clearMessages() {
    error.value = null
    errorContext.value = null
  }

  function applyPhase(next: AppUpdatePhase) {
    phase.value = next
    if (next !== 'installing') progress.value = null
  }

  /** Read the build's own answer first: it decides whether to offer one. */
  async function load() {
    declinedVersion.value = readDeclinedVersion()
    try {
      status.value = await readAppUpdateStatus()
    } catch (err) {
      fail('check', err)
      return
    }
    // The build already knows whether its key is a placeholder, and it
    // says so before any network call. Settle the phase here rather
    // than offering a Check button whose only possible outcome is a
    // refusal the user has to trigger to learn.
    if (!status.value.configured) {
      applyPhase('not-configured')
    }
  }

  async function check() {
    applyPhase('checking')
    clearMessages()
    detail.value = null
    notes.value = null
    publishedAt.value = null
    installed.value = null
    try {
      const result = await checkForAppUpdate(declinedVersion.value)
      offeredVersion.value = result.version
      notes.value = result.notes
      publishedAt.value = result.publishedAt
      detail.value = result.detail

      switch (result.status) {
        case 'available':
          applyPhase('available')
          break
        case 'declined':
          // A version the user already turned down is not re-offered, and
          // saying so is the whole of the "do not nag" behaviour.
          declinedVersion.value = result.version ?? declinedVersion.value
          applyPhase('declined')
          break
        case 'not-configured':
          applyPhase('not-configured')
          break
        case 'unavailable':
          applyPhase('unavailable')
          break
        default:
          applyPhase('up-to-date')
      }
    } catch (err) {
      applyPhase('idle')
      fail('check', err)
    }
  }

  /**
   * The user turned the offered version down.
   *
   * Records the version and returns to the plain "your version" state, so
   * the next check has something to send back and there is no update card
   * left on screen.
   */
  function decline() {
    const version = offeredVersion.value
    if (version) {
      declinedVersion.value = version
      rememberDeclined(version)
    }
    offeredVersion.value = null
    notes.value = null
    publishedAt.value = null
    detail.value = null
    clearMessages()
    applyPhase('idle')
  }

  /**
   * Change the answer, deliberately.
   *
   * A remembered refusal has to have a way back, or a stray Escape locks
   * the user out of a release until they clear their own storage. Nothing
   * is re-requested: the version and its notes are the ones the check
   * just read, and asking again would be a request the user did not make.
   */
  function showDeclinedAgain() {
    if (!offeredVersion.value) return
    declinedVersion.value = null
    forgetDeclined()
    clearMessages()
    applyPhase('available')
  }

  /**
   * Download, verify and install the version the user confirmed.
   *
   * Two things this deliberately does not do: it does not download unless
   * the confirmed version is the one on screen, and it does not treat a
   * resolved promise as success on Windows — the app is gone by then. The
   * progress event is what tells the user the install reached the
   * installer, and the "finishing" state is what tells them the restart
   * is the installer's doing, not a hang.
   */
  async function install() {
    const version = offeredVersion.value
    if (!version || phase.value !== 'available') return false

    applyPhase('installing')
    progress.value = { downloaded: 0, total: null }
    clearMessages()

    const unlisten = await onAppUpdateProgress((update) => {
      if (update.phase === 'installing') {
        applyPhase('finishing')
        return
      }
      progress.value = { downloaded: update.downloaded, total: update.total }
      if (update.total && update.downloaded >= update.total) applyPhase('finishing')
    })

    try {
      const result = await installAppUpdate(version)
      installed.value = result.version
      applyPhase('finishing')
      return true
    } catch (err) {
      // Every failure lands here, and the installed version is untouched
      // in all of them. The callout says which one it was.
      applyPhase('available')
      fail('install', err)
      return false
    } finally {
      unlisten()
    }
  }

  /** Download progress as a percentage, or `null` while it is unknown. */
  const percent = computed(() => {
    const current = progress.value
    if (!current?.total) return null
    return Math.min(100, Math.round((current.downloaded / current.total) * 100))
  })

  /** The same number, worded for the copy. */
  const percentLabel = computed(() => (percent.value === null ? null : `${percent.value}%`))

  const busy = computed(() => phase.value === 'checking' || phase.value === 'installing')

  return {
    phase,
    error,
    errorContext,
    status,
    offeredVersion,
    notes,
    publishedAt,
    detail,
    declinedVersion,
    percent,
    percentLabel,
    busy,
    load,
    check,
    decline,
    showDeclinedAgain,
    install,
  }
}
