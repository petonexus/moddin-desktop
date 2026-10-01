import type { ModuleCardState, ModuleCardUpdate } from '../library/ModuleCard.vue'
import type { ModuleCategory, ToolModuleDefinition } from '../../types/game'
import type { ModuleUpdate } from '../../types/module-update'
import type { ModuleVerification } from '../../types/module-verification'
import type { CompatibilityStatus } from '../../types/compatibility'
import { isCapabilityBackedModule, moduleDescriptionKey, moduleNameKey, type Translate } from './module-registry'

/**
 * What one module card shows, and what its one button does.
 *
 * The card itself renders state and knows nothing about the catalog, the
 * verification store or the transactions, so every decision about a
 * card used to live in `App.vue` next to the template that renders it.
 * The decisions are the interesting part — "is this card checking,
 * installed or blocked" is the answer to the question the player is
 * actually asking — so they live here, take a plain description of what
 * is true right now, and hand the card a finished view.
 *
 * Nothing here calls out. The caller passes the state it already has.
 */

/** The catalog order categories are offered in. */
const MODULE_CATEGORIES: ModuleCategory[] = ['vr', 'graphics', 'qol', 'system']

export type ModuleFilter = 'all' | ModuleCategory

export interface ModuleCardView {
  name: string
  description: string
  state: ModuleCardState
  tag: { label: string; tone: 'success' | 'warning' | 'danger' | 'neutral' } | null
  actionLabel: string
  actionPrimary: boolean
  actionBusy: boolean
  actionDisabled: boolean
  blockedReason?: string
  verification?: ModuleVerification
  checkedAtLabel?: string
  verifyBusy: boolean
  removeLabel: string | null
  update: ModuleCardUpdate | null
}

export interface ModuleCardViewInput {
  module: ToolModuleDefinition
  /** Last verification for this game/module, if one has run. */
  verification?: ModuleVerification
  /** A verification for this module is in flight right now. */
  verifying: boolean
  /** The game environment is being inspected, so verifications are stale. */
  loading: boolean
  /** Some other module action is holding the UI. */
  busy: boolean
  /** This module's own action is the one running. */
  actionBusy: boolean
  /** The selected game is running, which blocks every mutation. */
  gameRunning: boolean
  blockedReason?: string
  /** An applied transaction for this module is still on record. */
  installed: boolean
  update?: ModuleUpdate
  updateBusy: boolean
  /** The recipe declares somewhere to check for a newer version. */
  hasUpdateSource: boolean
  /** One-line update state for the card, already localized. */
  updateSummary: string
  /** Recorded (or recipe-declared) compatibility for this module. */
  compatibility: CompatibilityStatus
  t: Translate
  formatDate: (timestamp: number) => string
}

/**
 * `checking` wins over `unknown` only while something is actually
 * running. A module nobody has looked at yet is "unknown", not
 * "checking": the difference is whether the player can wait for an
 * answer that is coming.
 */
function cardState(input: ModuleCardViewInput): ModuleCardState {
  const { module, verification } = input
  if (module.status !== 'available') return 'planned'
  if (!verification) return input.loading || input.verifying ? 'checking' : 'unknown'
  if (verification.status === 'installed') return 'active'
  if (verification.status === 'ready') return 'available'
  if (verification.status === 'attention') return 'attention'
  return 'unknown'
}

/**
 * Show a compatibility chip for any module that carries an explicit
 * `compatibilityStatus` in its catalog config — not only Cheeky. Catalog
 * authors can mark research findings as `proven`, `experimental`,
 * `risky`, or `not_working` per-game; the chip surfaces that for the
 * player. Cheeky keeps its chip for entries that never gained the key,
 * so older recipes still show what the engine preset implies.
 */
function cardTag(input: ModuleCardViewInput) {
  const { module, compatibility, t } = input
  const hasExplicitCompat = Object.prototype.hasOwnProperty.call(module.config ?? {}, 'compatibilityStatus')
  if (!hasExplicitCompat && module.id !== 'cheeky-foveated-dlss') return null

  const tone: 'success' | 'warning' | 'danger' | 'neutral' = compatibility === 'proven'
    ? 'success'
    : compatibility === 'experimental'
      ? 'warning'
      : compatibility === 'unverified'
        ? 'neutral'
        : 'danger'
  return { label: compatibilityStatusLabel(compatibility, t), tone }
}

function compatibilityStatusLabel(status: CompatibilityStatus, t: Translate) {
  if (status === 'experimental') return t('compatibilityExperimental')
  if (status === 'proven') return t('compatibilityProven')
  if (status === 'risky') return t('compatibilityRisky')
  if (status === 'not_working') return t('compatibilityNotWorking')
  return t('compatibilityUnverified')
}

/**
 * Hide the whole update area when the recipe has nowhere to look:
 * showing "no update source" on every card was noise the user could not
 * act on.
 */
function cardUpdate(input: ModuleCardViewInput): ModuleCardUpdate | null {
  if (input.module.status !== 'available' || !input.hasUpdateSource) return null
  const available = input.update?.status === 'available'
  return {
    hasSource: true,
    available,
    summary: available
      ? input.t('updateAvailableShort', { version: input.update?.latestVersion ?? '?' })
      : input.updateSummary,
    releaseUrl: input.update?.releaseUrl ?? null,
    busy: input.updateBusy,
  }
}

/**
 * The card's one button. `actionBusy` is this module's own action, not
 * the global one: a card that is waiting says "Working…" while the
 * others stay clickable-looking but blocked.
 */
export function moduleActionLabel(input: ModuleCardViewInput) {
  const { module, verification, t } = input
  if (module.status !== 'available') return t('statePlanned')
  if (input.actionBusy) return t('actionOpening')
  if (module.id === 'vr-launch') return t('actionLaunchVr')
  if (module.id === 'desktop-shortcut') return t('actionCreateShortcut')
  if (verification?.status === 'installed') {
    return module.id === 'optiscaler' || module.id === 'uevr' ? t('actionReinstall') : t('actionApplyAgain')
  }
  return t('actionInstall')
}

/** Every mutation is refused while the game runs or the UI is busy. */
export function moduleActionsBlocked(input: ModuleCardViewInput) {
  return input.module.status !== 'available' || input.busy || input.loading || input.gameRunning
}

/** Once a mod is active, re-running it is a secondary gesture. */
function actionPrimary(input: ModuleCardViewInput) {
  return input.module.id === 'vr-launch' || input.verification?.status !== 'installed'
}

export function moduleCardView(input: ModuleCardViewInput): ModuleCardView {
  const { module, t, verification } = input
  return {
    name: t(moduleNameKey(module.id, module.name)),
    description: t(moduleDescriptionKey(module)),
    state: cardState(input),
    tag: cardTag(input),
    actionLabel: moduleActionLabel(input),
    actionPrimary: actionPrimary(input),
    actionBusy: input.actionBusy,
    actionDisabled: module.status !== 'available' || input.busy || input.loading || input.gameRunning,
    blockedReason: input.blockedReason,
    verification,
    checkedAtLabel: verification ? t('checkedAt', { date: input.formatDate(verification.checkedAt) }) : undefined,
    verifyBusy: input.verifying,
    removeLabel: module.status === 'available' && input.installed ? t('actionRemove') : null,
    update: cardUpdate(input),
  }
}

export interface ModuleGroup {
  category: ModuleCategory
  modules: ToolModuleDefinition[]
}

/**
 * Category tabs the selected game actually has something for. Counted
 * across every module it declares, including the ones the capability
 * section renders: a game whose only QoL entry is a capability recipe
 * still deserves the tab that shows it.
 */
export function availableModuleCategories(modules: readonly ToolModuleDefinition[]): ModuleCategory[] {
  return MODULE_CATEGORIES.filter((category) => modules.some((module) => module.category === category))
}

/**
 * Which modules the grid shows, grouped by category.
 *
 * `allModules` decides the category tabs; `ownedModules` is the subset
 * the grid itself can act on. A category with no owned module produces
 * no group, which is why the tabs can be longer than the list.
 */
export function selectModuleGroups(
  allModules: readonly ToolModuleDefinition[],
  ownedModules: readonly ToolModuleDefinition[],
  filter: ModuleFilter,
): ModuleGroup[] {
  return availableModuleCategories(allModules)
    .filter((category) => filter === 'all' || filter === category)
    .map((category) => ({
      category,
      modules: ownedModules.filter((module) => module.category === category),
    }))
    .filter((group) => group.modules.length > 0)
}

/** How many of the selected game's modules sit in a category, planned included. */
export function countModulesInCategory(modules: readonly ToolModuleDefinition[], category: ModuleCategory) {
  return modules.filter((module) => module.category === category).length
}

export interface ModuleProgress {
  total: number
  active: number
  attention: number
  checking: boolean
}

/**
 * The hero progress bar: how much of what the grid can complete is
 * working.
 *
 * The denominator is the grid's *own* available modules, not every
 * available module the game declares. The capability-backed ones
 * (`ofxr-bridge`, `reshade`, …) are rendered by the capability section,
 * which has its own progress of its own, and nothing the grid does can
 * ever move them off "unknown" — so counting them made the bar
 * unreachable: for S.T.A.L.K.E.R. 2 the total was five with only four
 * cards behind it, and installing everything the grid owns still left
 * the bar short. A bar that cannot reach its total is a broken promise,
 * not a motivation.
 */
export function summariseModuleProgress(
  modules: readonly ToolModuleDefinition[],
  stateOf: (module: ToolModuleDefinition) => ModuleCardState,
): ModuleProgress {
  const counted = modules
    .filter((module) => module.status === 'available' && !isCapabilityBackedModule(module))
    .map(stateOf)
  return {
    total: counted.length,
    active: counted.filter((state) => state === 'active').length,
    attention: counted.filter((state) => state === 'attention').length,
    checking: counted.includes('checking'),
  }
}
