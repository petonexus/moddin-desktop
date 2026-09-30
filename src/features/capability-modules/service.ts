import { invokeDebug as invoke } from '../../debug'
import { findCatalogGameById, findInstalledGameForCatalogGame } from '../../services/catalog'
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
import { capabilityCheckLabel, capabilityDescriptionKey } from '../../i18n/backendIds'
import type {
  CapabilityCompatibilityParams,
  CapabilityEvaluateParams,
  CapabilityInstallParams,
  CapabilityUninstallParams,
  CapabilityVerificationReport,
  CapabilityCheckOutcomeView,
  CapabilityVerificationReportView,
  CapabilitySummaryView,
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
 * Resolve the install target for a **catalog game id** (e.g.
 * `elden-ring`) — not a store app id.
 *
 * The community panel and the AI recommend flow both used to send empty
 * `installDir` / `executableDir`, which made every path-touching recipe
 * a no-op and silently disabled the `compatibility` gate. Callers that
 * cannot resolve a target should refuse the install instead of falling
 * back to blank paths.
 */
export async function resolveInstallTarget(
  catalogGameId: string | null,
): Promise<CapabilityInstallTarget | null> {
  if (!catalogGameId) return null
  const games = await invoke<InstalledGame[]>('detect_installed_games')
  // Every caller in the UI holds the catalog id. This used to match it
  // against `InstalledGame.appId` (a store id like `1245620`), which
  // never matches, so every install ended at "no game selected".
  const installed = findInstalledGameForCatalogGame(games, catalogGameId)
  if (!installed) return null
  const catalog = findCatalogGameById(catalogGameId)
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

/**
 * Summaries of every capability known to the runner.
 *
 * `engine` is the selected game's `enginePreset`, or null when the game
 * declares none — Elden Ring is the case that matters. The backend
 * decides what to do with it: a game with no engine gets everything, a
 * game whose engine no recipe names gets everything, and only a recipe
 * that names engines and not the game's is held back. That rule lives
 * in `CapabilitySpec::engine_match` because it is the only layer that
 * has the spec while the list is being built.
 *
 * Omitted rather than sent as null, so the argument shape stays
 * optional on the Tauri side and the callers that pass nothing still
 * typecheck.
 */
export function listCapabilities(engine?: string | null) {
  return invoke<CapabilitySummary[]>(
    'capability_list',
    engine ? { engine } : {},
  ).then((summaries) => summaries.map(toCapabilitySummaryView))
}

/**
 * UX-21: the card's `description` is a sentence this project wrote in a
 * recipe, and the recipes are English. A pt-BR user reading the mod
 * gallery was reading English to decide whether to install something.
 *
 * The key is attached here, next to the original string, and the section
 * renders the key when it is there. A recipe this table does not declare
 * — a community mod, one the AI saved — keeps the sentence its author
 * wrote; see the fallback note in `src/i18n/backendIds.ts`.
 *
 * Exported because it is the boundary itself: pure, and the one thing
 * worth asserting on without standing up the Tauri bridge.
 */
export function toCapabilitySummaryView(summary: CapabilitySummary): CapabilitySummaryView {
  const key = capabilityDescriptionKey(summary.id)
  return key ? { ...summary, descriptionKey: key } : { ...summary }
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
  return invoke<CapabilityCheckOutcome | null>('capability_compatibility', { request: params }).then(
    (outcome) => (outcome ? toCapabilityCheckView(outcome) : null),
  )
}

export function installCapability(params: CapabilityInstallParams) {
  return invoke<InstallResult>('capability_install', { request: params })
}

export function uninstallCapability(params: CapabilityUninstallParams) {
  return invoke<TransactionRecord>('capability_uninstall', { request: params })
}

export function evaluateCapability(params: CapabilityEvaluateParams) {
  return invoke<CapabilityVerificationReport>('capability_evaluate', { request: params }).then(
    (report) => ({
      ...report,
      checks: report.checks.map(toCapabilityCheckView),
    }),
  )
}

/**
 * UX-21: a check's `label` is the recipe author's sentence, and the
 * recipes this repository ships write it in English. The check **id** is
 * the identifier the backend sends and the table is keyed by it, so the
 * card can show a translated label without ever seeing the id.
 *
 * An id with no entry keeps the recipe's own label — that string is the
 * author's copy, not the backend's, and a community recipe's check
 * should read the way its author wrote it.
 */
export function toCapabilityCheckView(outcome: CapabilityCheckOutcome): CapabilityCheckOutcomeView {
  const key = capabilityCheckLabel(outcome.id)
  return key ? { ...outcome, labelKey: key } : { ...outcome }
}

export type { ResolvedConfig }
