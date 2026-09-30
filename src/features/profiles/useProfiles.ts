import { computed, ref } from 'vue'
import { version as appVersion } from '../../../package.json'
import { findCatalogGameById } from '../../services/catalog'
import { installCapability } from '../capability-modules/service'
import type { CapabilityConfigValue } from '../capability-modules/types'
import {
  listTransactions,
  loadProfileContext,
  readProfileFile,
  revealProfileFile,
  writeProfileFile,
} from './service'
import type { ProfileFileResult } from './service'
import {
  PROFILE_SCHEMA_VERSION,
  applicableEntries,
  buildProfileDocument,
  installedCapabilitiesByGame,
  parseProfileDocument,
  planProfileImport,
  type ModdinProfileDocument,
  type ProfileGameTarget,
  type ProfileImportPlan,
  type ProfileParseErrorCode,
} from './types'

/** Which action produced `error`; the raw string alone cannot say. */
export type ProfileAction = 'export' | 'import' | 'apply' | 'reveal'

/** A refusal the view already knows how to phrase. */
export interface ProfileNotice {
  code: ProfileParseErrorCode | 'export-empty'
  detail: string
}

export interface ProfileApplyResult {
  installed: string[]
  failed: Array<{ id: string; message: string }>
}

/**
 * How the host answers "what config does this capability have for this
 * game right now?".
 *
 * Resolved config is not persisted anywhere in the app: the capability
 * card holds it in memory and the transaction log records only the id
 * and the resolved `version`. A profile can therefore carry the real
 * settings only for a game the library view currently has open; for
 * every other game it carries the recipe defaults, and the export
 * preview has to say so — a silently defaulted field is
 * indistinguishable from one the user chose.
 */
export interface ProfileConfigLookup {
  values: (gameId: string, capabilityId: string) => Record<string, CapabilityConfigValue> | undefined
}

const noConfigLookup: ProfileConfigLookup = { values: () => undefined }

export interface UseProfilesOptions {
  configFor?: ProfileConfigLookup
}

export function useProfiles(options: UseProfilesOptions = {}) {
  const configFor = options.configFor ?? noConfigLookup

  const busy = ref(false)
  const error = ref<string | null>(null)
  const errorContext = ref<ProfileAction | null>(null)
  const notice = ref<ProfileNotice | null>(null)

  const fileName = ref(defaultFileName())
  const exportResult = ref<ProfileFileResult | null>(null)

  const importPath = ref('')
  const loadedPath = ref<string | null>(null)
  const preview = ref<ProfileImportPlan | null>(null)
  const applyResult = ref<ProfileApplyResult | null>(null)
  const applying = ref(false)

  /**
   * The targets resolved while planning. Kept so applying does not
   * re-resolve the install directory per capability — `resolveInstallTarget`
   * scans the store, and the answer cannot have changed between the
   * preview the user read and the button they pressed.
   */
  let targets = new Map<string, ProfileGameTarget | null>()

  function fail(action: ProfileAction, err: unknown) {
    error.value = err instanceof Error ? err.message : String(err)
    errorContext.value = action
  }

  function clearMessages() {
    error.value = null
    errorContext.value = null
    notice.value = null
  }

  function defaultFileName(): string {
    return `moddin-profile-${new Date().toISOString().slice(0, 10)}.json`
  }

  /**
   * Everything Moddin considers installed, as one document.
   *
   * A transaction whose `kind` is not in the registry has no recipe, so
   * it is left out rather than written as an entry no import could ever
   * act on. The export preview lists exactly what went in, which is why
   * a partial export is honest instead of surprising.
   */
  async function buildExport(): Promise<ModdinProfileDocument> {
    const transactions = await listTransactions()
    const installedByGame = installedCapabilitiesByGame(transactions)
    const capabilityIds = [...new Set([...installedByGame.values()].flatMap((ids) => [...ids]))]

    const context = await loadProfileContext({ gameIds: [], capabilityIds, transactions })

    const games = [...installedByGame.keys()]
      .sort()
      .map((gameId) => ({
        gameId,
        gameName: findCatalogGameById(gameId)?.name ?? gameId,
        capabilities: [...(installedByGame.get(gameId) ?? [])]
          .filter((id) => context.capabilities.has(id))
          .sort()
          .map((id) => ({
            id,
            displayName: context.capabilities.get(id)?.displayName || id,
            spec: context.specs.get(id) ?? null,
            values: configFor.values(gameId, id) ?? {},
          })),
      }))
      .filter((game) => game.capabilities.length > 0)

    return buildProfileDocument({
      appVersion,
      exportedAt: new Date().toISOString(),
      games,
    })
  }

  const canExport = computed(
    () => fileName.value.trim().toLowerCase().endsWith('.json') && fileName.value.trim().length > 5,
  )
  const canReadImport = computed(() => importPath.value.trim().length > 0)
  const canApply = computed(
    () => Boolean(preview.value) && (preview.value?.applyCount ?? 0) > 0 && !applying.value,
  )

  async function runExport() {
    if (!canExport.value) return false
    busy.value = true
    clearMessages()
    exportResult.value = null
    try {
      const document = await buildExport()
      if (!document.games.length) {
        notice.value = { code: 'export-empty', detail: '' }
        return false
      }
      exportResult.value = await writeProfileFile(
        fileName.value.trim(),
        `${JSON.stringify(document, null, 2)}\n`,
      )
      return true
    } catch (err) {
      fail('export', err)
      return false
    } finally {
      busy.value = false
    }
  }

  /**
   * Read a profile and produce the plan. Nothing is applied here and
   * there is no way to apply it from this function: the only way past
   * the preview is `applyPreview`.
   */
  async function readImport() {
    if (!canReadImport.value) return false
    busy.value = true
    clearMessages()
    applyResult.value = null
    preview.value = null
    loadedPath.value = null
    targets = new Map()
    try {
      const file = await readProfileFile(importPath.value.trim())
      const parsed = parseProfileDocument(file.contents)
      if (!parsed.ok) {
        notice.value = { code: parsed.code, detail: parsed.detail }
        return false
      }

      const document = parsed.document
      const context = await loadProfileContext({
        gameIds: document.games.map((game) => game.gameId),
        capabilityIds: document.games.flatMap((game) => game.capabilities.map((entry) => entry.id)),
      })
      targets = context.targets
      preview.value = planProfileImport(document, context)
      loadedPath.value = file.path
      return true
    } catch (err) {
      fail('import', err)
      return false
    } finally {
      busy.value = false
    }
  }

  /** Forget the loaded profile. Applies nothing; that is the point. */
  function cancelImport() {
    preview.value = null
    loadedPath.value = null
    applyResult.value = null
    targets = new Map()
    clearMessages()
  }

  /**
   * Apply the plan `readImport` produced.
   *
   * Every entry goes through `capability_install` — the same command the
   * capability card uses, so the preflight checks, the transaction and
   * the rollback are the ones Moddin already has. There is no second
   * install path here, and a blocked entry is never reached: the
   * preview already refused it and this walks the same plan the user
   * read.
   *
   * A failure stops the run and reports what already went in. Each
   * install is its own transaction, so each one can be undone from
   * History on its own.
   */
  async function applyPreview(
    onChanged?: () => void | Promise<void>,
  ): Promise<ProfileApplyResult | null> {
    const plan = preview.value
    if (!plan || !canApply.value) return null

    applying.value = true
    busy.value = true
    clearMessages()
    const result: ProfileApplyResult = { installed: [], failed: [] }
    try {
      for (const { game, capability } of applicableEntries(plan)) {
        const target = targets.get(game.gameId)
        if (!target) {
          result.failed.push({ id: capability.id, message: 'game-not-installed' })
          break
        }
        try {
          await installCapability({
            capabilityId: capability.id,
            gameId: target.gameId,
            gameName: target.gameName,
            installDir: target.installDir,
            executableDir: target.executableDir,
            config: { values: capability.config },
          })
          result.installed.push(capability.id)
        } catch (err) {
          result.failed.push({
            id: capability.id,
            message: err instanceof Error ? err.message : String(err),
          })
          break
        }
      }
      applyResult.value = result
      if (result.installed.length) await onChanged?.()
      return result
    } catch (err) {
      fail('apply', err)
      return null
    } finally {
      applying.value = false
      busy.value = false
    }
  }

  async function reveal() {
    const path = exportResult.value?.path
    if (!path) return
    busy.value = true
    clearMessages()
    try {
      await revealProfileFile(path)
    } catch (err) {
      fail('reveal', err)
    } finally {
      busy.value = false
    }
  }

  function reset() {
    cancelImport()
    exportResult.value = null
    fileName.value = defaultFileName()
  }

  return {
    fileName,
    exportResult,
    importPath,
    loadedPath,
    preview,
    applyResult,
    applying,
    busy,
    error,
    errorContext,
    notice,
    canExport,
    canReadImport,
    canApply,
    schemaVersion: PROFILE_SCHEMA_VERSION,
    runExport,
    readImport,
    applyPreview,
    cancelImport,
    reveal,
    reset,
  }
}
