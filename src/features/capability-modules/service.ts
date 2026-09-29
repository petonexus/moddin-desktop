import { invokeDebug as invoke } from '../../debug'
import { findCatalogGameByInstalledGame } from '../../services/catalog'
import type { GameEnvironmentInspection } from '../../types/inspection'
import type { InstalledGame } from '../../types/game'
import type {
  CapabilityCheckOutcome,
  CapabilitySpec,
  CapabilitySummary,
  InstallResult,
  ResolvedConfig,
} from '../../types/capability'
import type { TransactionRecord } from '../../types/transaction'
import type {
  CapabilityCompatibilityParams,
  CapabilityEvaluateParams,
  CapabilityInstallParams,
  CapabilityUninstallParams,
  CapabilityVerificationReport,
} from './types'

/**
 * Everything a capability install needs to touch a real game folder.
 * `gameId` is the catalog id (the same value the per-game module cards
 * use), so a transaction recorded from the community panel or the AI
 * recommend flow is indistinguishable from one recorded in the game
 * detail view.
 */
export interface CapabilityInstallTarget {
  gameId: string
  gameName: string
  installDir: string
  executableDir: string
}

/**
 * Resolve the install target for a store app id.
 *
 * The community panel and the AI recommend flow both used to send empty
 * `installDir` / `executableDir`, which made every path-touching recipe
 * a no-op and silently disabled the `compatibility` gate. Callers that
 * cannot resolve a target should refuse the install instead of falling
 * back to blank paths.
 */
export async function resolveInstallTarget(appId: string | null): Promise<CapabilityInstallTarget | null> {
  if (!appId) return null
  const games = await invoke<InstalledGame[]>('detect_installed_games')
  const installed = games.find((game) => game.appId === appId)
  if (!installed) return null
  const catalog = findCatalogGameByInstalledGame(installed)
  if (!catalog) return null

  // The executable directory is resolved by the backend, not guessed
  // from the install dir: catalog executables are relative paths that
  // may point into a nested folder.
  const inspection = await invoke<GameEnvironmentInspection>('inspect_game_environment', {
    installDir: installed.installDir,
    executable: catalog.executable,
  })

  return {
    gameId: catalog.id,
    gameName: catalog.name,
    installDir: installed.installDir,
    executableDir: inspection.executableDirectory,
  }
}

export function listCapabilities() {
  return invoke<CapabilitySummary[]>('capability_list')
}

/** Full recipe for one capability (config schema, safety notes, checks). */
export function getCapabilitySpec(capabilityId: string) {
  return invoke<CapabilitySpec>('capability_get', { request: { capabilityId } })
}

/**
 * Probe only the spec's `compatibility` block. Resolves to `null` when
 * the spec supports every game build — cheaper than `evaluateCapability`,
 * which also downloads archives for the `archive-sha256` check.
 */
export function getCapabilityCompatibility(params: CapabilityCompatibilityParams) {
  return invoke<CapabilityCheckOutcome | null>('capability_compatibility', { request: params })
}

export function installCapability(params: CapabilityInstallParams) {
  return invoke<InstallResult>('capability_install', { request: params })
}

export function uninstallCapability(params: CapabilityUninstallParams) {
  return invoke<TransactionRecord>('capability_uninstall', { request: params })
}

export function evaluateCapability(params: CapabilityEvaluateParams) {
  return invoke<CapabilityVerificationReport>('capability_evaluate', { request: params })
}

export type { ResolvedConfig }
