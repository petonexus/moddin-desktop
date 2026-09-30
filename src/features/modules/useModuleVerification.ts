import { ref, type Ref } from 'vue'
import type { ModuleUpdate } from '../../types/module-update'
import type { ModuleVerification, ModuleVerificationCheck } from '../../types/module-verification'
import type { ToolModuleDefinition } from '../../types/game'
import type { DesktopShortcutPreview } from '../desktop-shortcut/types'
import { checkModuleUpdate as checkModuleUpdateCommand } from './service'
import {
  libraryModuleEntry,
  moduleHasUpdateSource,
  moduleNameKey,
  resolveModuleUpdateSource,
  type ModuleEntry,
  type ModuleGame,
  type ModuleRequestContext,
  type Translate,
} from './module-registry'
import { verificationSummary } from './verification-probes'

/**
 * Verification and update checks for the selected game's modules.
 *
 * Both are the same shape of work: run something in the background for
 * every module, remember the answer per (game, module), and drop it if
 * the player moved to another game while it was in flight. That last
 * part is why this owns a *generation* counter rather than comparing app
 * ids: switching from one game to another and back inside one debounce
 * window produces the same app id twice, and the stale answer for the
 * first visit would overwrite the fresh one for the second.
 *
 * The per-module knowledge — which command, which request, which
 * checklist — is in `module-registry.ts` and `verification-probes.ts`.
 * What is here is the scheduling, the bookkeeping and the staleness
 * rules, none of which any single module owns.
 */

/** How long a verification stays fresh before the background pass redoes it. */
const AUTO_VERIFICATION_TTL_MS = 15_000
/** Update checks are slower and change less often than a local check. */
const AUTO_UPDATE_TTL_MS = 10 * 60_000
/** A hung release API must not hold a card's spinner forever. */
const UPDATE_CHECK_TIMEOUT_MS = 35_000
/** Two at a time: enough to overlap, few enough not to thrash the disk. */
const BACKGROUND_CONCURRENCY = 2

export interface ModuleVerificationHost {
  t: Translate
  selectedAppId: () => string | null
  /** Incremented on every selection change; work from an older one is dropped. */
  selectionGeneration: () => number
  gameContext: (appId: string | null) => ModuleGame | null
  /** Everything a module's request builder is allowed to read. */
  requestContext: (module: ToolModuleDefinition) => ModuleRequestContext
  /** The key this module's state is remembered under, for one app id. */
  stateKey: (module: ToolModuleDefinition, appId: string | null) => string
  /** Applied transaction for this module, if the store has one. */
  activeTransactionId: (module: ToolModuleDefinition) => string | null
  /**
   * The desktop shortcut is the one module whose preview belongs to
   * another feature, so the grid asks for it instead of calling a command.
   */
  desktopShortcutPreview: (module: ToolModuleDefinition) => Promise<DesktopShortcutPreview>
  actionError: Ref<string | null>
  success: Ref<string | null>
}

function messageOf(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

/**
 * Reject with `message` if `promise` has not settled by then. The timer
 * is cleared on both paths, so a slow answer never keeps a handle alive.
 */
function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error(message)), timeoutMs)
    promise.then(resolve, reject).finally(() => window.clearTimeout(timer))
  })
}

async function runWithConcurrency<T>(
  items: T[],
  concurrency: number,
  worker: (item: T) => Promise<void>,
) {
  let nextIndex = 0
  const runners = Array.from({ length: Math.min(concurrency, items.length) }, async () => {
    while (nextIndex < items.length) {
      const item = items[nextIndex]
      nextIndex += 1
      if (item !== undefined) await worker(item)
    }
  })
  await Promise.all(runners)
}

/**
 * The three modules whose installed version only the backend knows, and
 * therefore cannot be read from the catalog config the way the others
 * are. Each preview reports it; the rest have nothing to compare the
 * release against.
 */
async function installedVersionFromPreview(
  entry: ModuleEntry,
  context: ModuleRequestContext,
  t: Translate,
): Promise<string | null> {
  switch (entry.dialog) {
    case 'optiscaler': {
      const request = entry.buildRequest(context)
      if (!request) return null
      const preview = await withTimeout(entry.preview(request), UPDATE_CHECK_TIMEOUT_MS, t('updateFailed'))
      return preview.installedVersion ?? request.version
    }
    case 'ofxr': {
      const request = entry.buildRequest(context)
      if (!request) return null
      const preview = await withTimeout(entry.preview(request), UPDATE_CHECK_TIMEOUT_MS, t('updateFailed'))
      return preview.installedVersion ?? request.version
    }
    case 'cheeky': {
      const request = entry.buildRequest(context)
      if (!request) return null
      const preview = await withTimeout(entry.preview(request), UPDATE_CHECK_TIMEOUT_MS, t('updateFailed'))
      return preview.installedVersion ?? request.version
    }
    default:
      return null
  }
}

export function useModuleVerification(host: ModuleVerificationHost) {
  const verifications = ref<Record<string, ModuleVerification>>({})
  const updates = ref<Record<string, ModuleUpdate>>({})
  /** The card whose verify button the player pressed, for the single spinner. */
  const verifyingKey = ref<string | null>(null)
  const verifyingKeys = ref(new Set<string>())
  const updateBusyKeys = ref(new Set<string>())

  function current(expectedAppId: string | null, expectedGeneration: number) {
    return host.selectedAppId() === expectedAppId && host.selectionGeneration() === expectedGeneration
  }

  function verificationFor(module: ToolModuleDefinition) {
    return verifications.value[host.stateKey(module, host.selectedAppId())]
  }

  function isVerifying(module: ToolModuleDefinition) {
    const key = host.stateKey(module, host.selectedAppId())
    return verifyingKey.value === key || verifyingKeys.value.has(key)
  }

  function updateFor(module: ToolModuleDefinition) {
    return updates.value[host.stateKey(module, host.selectedAppId())]
  }

  function isUpdateBusy(module: ToolModuleDefinition) {
    return updateBusyKeys.value.has(host.stateKey(module, host.selectedAppId()))
  }

  function saveVerification(
    module: ToolModuleDefinition,
    expectedAppId: string | null,
    expectedGeneration: number,
    status: ModuleVerification['status'],
    summary: string,
    checks: ModuleVerificationCheck[],
    gameRunning: boolean,
  ) {
    if (!current(expectedAppId, expectedGeneration) || !expectedAppId) return
    verifications.value[host.stateKey(module, expectedAppId)] = {
      status,
      summary,
      checks,
      gameRunning,
      checkedAt: Date.now(),
      activeTransactionId: host.activeTransactionId(module),
    }
  }

  /**
   * Preview one module and record what the preview says.
   *
   * A module with no table row records nothing and still reports success:
   * the card shows "unknown" and the player can verify it by hand. That
   * is the honest answer for a recipe Moddin cannot check, and it is why
   * the catalogue test insists every available module *does* have a row.
   */
  async function verifyModule(
    module: ToolModuleDefinition,
    silent = false,
    expectedAppId = host.selectedAppId(),
    expectedGeneration = host.selectionGeneration(),
  ) {
    if (module.status !== 'available') return
    if (!current(expectedAppId, expectedGeneration)) return

    const key = host.stateKey(module, expectedAppId)
    const busyKeys = new Set(verifyingKeys.value)
    busyKeys.add(key)
    verifyingKeys.value = busyKeys
    if (!silent) verifyingKey.value = key
    if (!silent) host.actionError.value = null

    try {
      const entry = libraryModuleEntry(module.id)
      if (entry) {
        const outcome = await entry.verify(host.requestContext(module), host.t)
        saveVerification(
          module,
          expectedAppId,
          expectedGeneration,
          outcome.status,
          outcome.summary,
          outcome.checks,
          outcome.gameRunning,
        )
      } else if (module.id === 'desktop-shortcut') {
        // The shortcut's preview comes from its own feature; the grid only
        // records whether it could be created.
        const preview = await host.desktopShortcutPreview(module)
        const status: ModuleVerification['status'] = preview.canApply ? 'ready' : 'attention'
        saveVerification(
          module,
          expectedAppId,
          expectedGeneration,
          status,
          verificationSummary(status, host.t),
          [{ label: host.t('checkExecutable'), passed: preview.canApply }],
          false,
        )
      }

      if (!silent && current(expectedAppId, expectedGeneration)) {
        host.success.value = host.t('verificationCompleted', {
          module: host.t(moduleNameKey(module.id, module.name)),
        })
      }
    } catch (error) {
      if (!silent && current(expectedAppId, expectedGeneration)) {
        host.actionError.value = messageOf(error)
      }
    } finally {
      const nextBusyKeys = new Set(verifyingKeys.value)
      nextBusyKeys.delete(key)
      verifyingKeys.value = nextBusyKeys
      if (!silent && verifyingKey.value === key) verifyingKey.value = null
    }
  }

  /**
   * Verify every module whose answer is missing or older than the TTL.
   *
   * UEVR is skipped, as it has always been, which makes it the one
   * available card that sits on "unknown" until the player presses
   * Verify. Whether that is deliberate is worth a second look; it is
   * called out here so the next reader does not read the skip as
   * obvious, and because the skip is invisible from the card itself.
   */
  async function verifyAvailableModules(
    expectedAppId = host.selectedAppId(),
    expectedGeneration = host.selectionGeneration(),
    force = false,
  ) {
    const game = host.gameContext(expectedAppId)
    if (!game || !current(expectedAppId, expectedGeneration) || !expectedAppId) return
    const now = Date.now()
    const modules = game.catalog.modules.filter((module) => {
      if (module.status !== 'available' || module.id === 'uevr') return false
      const previous = verifications.value[host.stateKey(module, expectedAppId)]
      return force || !previous || now - previous.checkedAt > AUTO_VERIFICATION_TTL_MS
    })
    await runWithConcurrency(modules, BACKGROUND_CONCURRENCY, async (module) => {
      await verifyModule(module, true, expectedAppId, expectedGeneration)
    })
  }

  function hasUpdateSource(module: ToolModuleDefinition) {
    return moduleHasUpdateSource(module, host.requestContext(module))
  }

  /** One line about the update state, for the card. */
  function updateSummary(module: ToolModuleDefinition) {
    const update = updateFor(module)
    if (!hasUpdateSource(module)) return host.t('updateNoSource')
    if (!update) return host.t('updateNotChecked')
    if (update.status === 'available') {
      return host.t('updateAvailableSummary', { version: update.latestVersion ?? '?' })
    }
    if (update.status === 'current') {
      return host.t('updateCurrentSummary', { version: update.latestVersion ?? update.currentVersion ?? '?' })
    }
    if (update.status === 'unavailable') return host.t('updateNoSource')
    if (update.status === 'error') return host.t('updateFailed')
    if (update.status === 'unknown' && update.latestVersion) {
      return host.t('updateUnknownLocalSummary', { version: update.latestVersion })
    }
    return host.t('updateUnknownLocal')
  }

  async function checkModuleUpdate(
    module: ToolModuleDefinition,
    silent = false,
    expectedAppId = host.selectedAppId(),
    expectedGeneration = host.selectionGeneration(),
  ) {
    if (module.status !== 'available') return
    if (!current(expectedAppId, expectedGeneration) || !expectedAppId) return

    const key = host.stateKey(module, expectedAppId)
    if (updateBusyKeys.value.has(key)) return
    const config = module.config ?? {}
    const configuredVersion = typeof config.version === 'string' ? config.version : null
    const updateUrl = resolveModuleUpdateSource(module, host.requestContext(module))
    if (!updateUrl) {
      // A recipe with nowhere to look is not a failure to report, it is a
      // card that should not have offered the check.
      updates.value[key] = {
        status: 'unavailable',
        currentVersion: configuredVersion,
        latestVersion: null,
        releaseUrl: null,
        checkedAt: Date.now(),
        detail: 'This module recipe does not define an update source.',
      }
      return
    }

    const busyKeys = new Set(updateBusyKeys.value)
    busyKeys.add(key)
    updateBusyKeys.value = busyKeys
    let currentVersion = configuredVersion

    try {
      const entry = libraryModuleEntry(module.id)
      if (entry) {
        currentVersion = await installedVersionFromPreview(entry, host.requestContext(module), host.t) ?? currentVersion
      }
      if (!current(expectedAppId, expectedGeneration)) return

      const result = await withTimeout(
        checkModuleUpdateCommand({ currentVersion, updateUrl }),
        UPDATE_CHECK_TIMEOUT_MS,
        host.t('updateFailed'),
      )
      if (current(expectedAppId, expectedGeneration)) {
        updates.value[key] = { ...result, checkedAt: Date.now() }
      }
    } catch (error) {
      if (current(expectedAppId, expectedGeneration)) {
        updates.value[key] = {
          status: 'error',
          currentVersion,
          latestVersion: null,
          releaseUrl: null,
          checkedAt: Date.now(),
          detail: messageOf(error),
        }
        if (!silent) host.actionError.value = messageOf(error)
      }
    } finally {
      const nextBusyKeys = new Set(updateBusyKeys.value)
      nextBusyKeys.delete(key)
      updateBusyKeys.value = nextBusyKeys
    }
  }

  /** The same, for every module with an update source, behind the TTL. */
  async function checkAvailableModuleUpdates(
    expectedAppId = host.selectedAppId(),
    expectedGeneration = host.selectionGeneration(),
  ) {
    const game = host.gameContext(expectedAppId)
    if (!game || !current(expectedAppId, expectedGeneration) || !expectedAppId) return
    const now = Date.now()
    const modules = game.catalog.modules.filter((module) => {
      if (module.status !== 'available' || !hasUpdateSource(module)) return false
      const previous = updates.value[host.stateKey(module, expectedAppId)]
      return !previous || now - previous.checkedAt > AUTO_UPDATE_TTL_MS
    })
    await runWithConcurrency(modules, BACKGROUND_CONCURRENCY, async (module) => {
      await checkModuleUpdate(module, true, expectedAppId, expectedGeneration)
    })
  }

  return {
    verifications,
    updates,
    verificationFor,
    isVerifying,
    verifyModule,
    verifyAvailableModules,
    updateFor,
    isUpdateBusy,
    updateSummary,
    hasUpdateSource,
    checkModuleUpdate,
    checkAvailableModuleUpdates,
  }
}
