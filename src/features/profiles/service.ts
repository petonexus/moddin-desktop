import { invokeDebug as invoke } from '../../debug'
import { findCatalogGameById } from '../../services/catalog'
import type { CapabilitySpec, CapabilitySummary } from '../../types/capability'
import type { GameEnvironmentInspection } from '../../types/inspection'
import type { TransactionRecord } from '../../types/transaction'
import {
  getCapabilitySpec,
  listCapabilities as listRegistryCapabilities,
  resolveInstallTarget,
} from '../capability-modules/service'
import { fetchCommunityCatalog } from '../community/service'
import { installedCapabilitiesByGame } from './types'
import type { ProfileGameTarget, ProfileImportContext } from './types'

/** Mirrors `crate::profile_files::ProfileFileResult`. */
export interface ProfileFileResult {
  path: string
  bytes: number
}

/** Mirrors `crate::profile_files::ProfileFileContents`. */
export interface ProfileFileContents {
  path: string
  contents: string
}

/**
 * Ask the user where the profile goes, and write it there.
 *
 * `fileName` is the name the dialog starts with, not a destination: the
 * path the file lands on is the user's answer, and the command writes
 * nothing at all when they close the window — which is why the return
 * type is nullable. `null` here is "no file was written", never a
 * failure, and a caller that treats it as one is the bug this signature
 * is shaped to prevent.
 */
export function saveProfileFile(fileName: string, contents: string) {
  return invoke<ProfileFileResult | null>('save_profile_file', { request: { fileName, contents } })
}

/**
 * Ask which profile file to read. `null` is the user closing the
 * dialog.
 *
 * The command answers with a path and nothing else; the file is read by
 * `readProfileFile`, so a file chosen in the dialog and a path typed in
 * the box are judged by the same rules.
 */
export function pickProfileFile() {
  return invoke<string | null>('pick_profile_file')
}

/** Read a profile from a path the user names, anywhere on the disk. */
export function readProfileFile(path: string) {
  return invoke<ProfileFileContents>('read_profile_file', { request: { path } })
}

/** Show the exported file in Explorer/Finder/Files. */
export function revealProfileFile(path: string) {
  return invoke<null>('reveal_profile_file', { request: { path } })
}

/**
 * The transaction log is what "installed" means everywhere else in
 * Moddin — the capability cards, the history view and the backend's own
 * `is_capability_installed` all read the same records — so an export
 * derives the installed set from here rather than keeping a second list
 * that could disagree with it.
 */
export function listTransactions() {
  return invoke<TransactionRecord[]>('list_transactions')
}

/**
 * Which capabilities are installed for which game, read off the
 * transaction log.
 *
 * Re-exported from `types` so a caller only has to know about the
 * feature's one service boundary.
 */
export { installedCapabilitiesByGame } from './types'

/** Mirrors `crate::inspection::inspect_game_environment`. */
export function inspectGameEnvironment(installDir: string, executable: string) {
  return invoke<GameEnvironmentInspection>('inspect_game_environment', {
    installDir,
    executable,
  })
}

export interface ProfileContextRequest {
  /** Games the preview has to resolve. An empty list resolves none. */
  gameIds: string[]
  /** Capabilities whose full recipe (`capability_get`) is needed. */
  capabilityIds: string[]
  /**
   * Pass the transactions when the caller already has them, so a flow
   * that needs the installed set before it can name the capabilities
   * does not read the log twice.
   */
  transactions?: TransactionRecord[]
}

/**
 * Everything the preview needs to judge a profile, gathered once so
 * reading a file never waits on a capability fetch.
 *
 * Each piece reuses a command the app already calls rather than adding a
 * parallel query: the transaction log, `capability_list`, and
 * `resolve_install_target` — the same target the install card builds.
 * The two additions are `capability_get` (the summaries carry no
 * `supportedEngines` or `configSchema`, and the preview needs both) and
 * one engine probe, because `resolveInstallTarget` does not return the
 * engine and the engine is what `supportedEngines` is checked against.
 */
export async function loadProfileContext(
  request: ProfileContextRequest,
): Promise<ProfileImportContext> {
  const [transactions, summaries, community] = await Promise.all([
    request.transactions ? Promise.resolve(request.transactions) : listTransactions(),
    listRegistryCapabilities(),
    // Cached only: reading a profile must never cause a network fetch.
    // A kill switch that could not be checked leaves the community
    // capabilities blocked, which is the stance the Community panel
    // already takes for the same reason.
    fetchCommunityCatalog(false, 0).catch(() => null),
  ])

  const capabilities = new Map(summaries.map((summary) => [summary.id, summary]))

  const specs = new Map<string, CapabilitySpec>()
  await Promise.all(
    [...new Set(request.capabilityIds)]
      .filter((id) => capabilities.has(id))
      .map(async (id) => {
        try {
          specs.set(id, await getCapabilitySpec(id))
        } catch {
          // A recipe that will not load is not a reason to fail the
          // whole preview: the plan reports the capability as unusable.
        }
      }),
  )

  const targets = new Map<string, ProfileGameTarget | null>()
  await Promise.all(
    [...new Set(request.gameIds)].map(async (gameId) => {
      const target = await resolveInstallTarget(gameId)
      if (!target) {
        targets.set(gameId, null)
        return
      }
      const catalog = findCatalogGameById(gameId)
      const inspection = catalog
        ? await inspectGameEnvironment(target.installDir, catalog.executable).catch(() => null)
        : null
      targets.set(gameId, {
        gameId: target.gameId,
        gameName: target.gameName,
        installDir: target.installDir,
        executableDir: target.executableDir,
        engine: inspection?.engine ?? null,
        engineVersion: inspection?.engineVersion ?? null,
      })
    }),
  )

  const revoked = new Map<string, string>()
  // The revocation list rides inside the signed catalog, so the catalog's
  // signature is also what makes it trustworthy — there is no longer a
  // second `revocationsVerified` fact to check.
  if (community?.signatureVerified) {
    for (const entry of community.catalog.revoked) revoked.set(entry.id, entry.reason)
  }

  return {
    capabilities,
    specs,
    installedByGame: installedCapabilitiesByGame(transactions),
    targets,
    revoked,
  }
}

