<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invokeDebug as invoke } from './debug'
import { findCatalogGameByInstalledGame, gameCatalog, getMissingDependenciesForModule } from './services/catalog'
import { localeOptions } from './i18n'
import type { InstalledGame, ToolModuleDefinition } from './types/game'
import type { GameEnvironmentInspection } from './types/inspection'
import type { ObsVrPreview, ObsVrRequest } from './types/obs'
import type { OptiScalerPreview, OptiScalerRequest } from './types/optiscaler'
import type { OfxrPreview, OfxrRequest } from './types/ofxr'
import type { CheekyFoveatedDlssPreview, CheekyFoveatedDlssRequest } from './types/cheeky'
import type { UevrBackend, UevrBackendCompatibility, UevrPreview, UevrRequest } from './types/uevr'
import type { TransactionRecord } from './types/transaction'
import type { VrLaunchPreview, VrLaunchRequest } from './types/vr-launch'
import DesktopShortcutDialog from './features/desktop-shortcut/DesktopShortcutDialog.vue'
import AiAssistantDialog from './components/shell/AiAssistantDialog.vue'
import AiGameSuggestions from './components/shell/AiGameSuggestions.vue'
import AiTopbarMenu from './components/shell/AiTopbarMenu.vue'
import { useAiTopbarActions } from './composables/useAiTopbarActions'
import { useDesktopShortcut } from './features/desktop-shortcut/useDesktopShortcut'
import { version as appVersion } from '../package.json'
import AppIcon from './components/ui/AppIcon.vue'
import BaseDialog from './components/ui/BaseDialog.vue'
import ChangePreview from './components/ui/ChangePreview.vue'
import EmptyState from './components/ui/EmptyState.vue'
import ToastStack from './components/ui/ToastStack.vue'
import GlobalTools from './components/shell/GlobalTools.vue'
import GameList from './features/library/GameList.vue'
import ModuleCard from './features/library/ModuleCard.vue'
import HistoryView from './features/history/HistoryView.vue'
import CapabilityModulesSection from './features/capability-modules/CapabilityModulesSection.vue'
import {
  configureObsVr,
  installCheekyFoveatedDlss,
  installOfxr,
  installOptiScaler,
  installUevr,
  launchVrGame as launchVrGameCommand,
  previewOptiScaler,
  rollbackLatestModuleTransaction,
} from './features/modules/service'
import {
  buildOfxrRequest,
  cheekyCompatibilityNote,
  configList,
  libraryModuleEntry,
  libraryOwnedModules,
  moduleDescriptionKey,
  moduleNameKey,
  moduleStateKey,
  moduleTransactionKind,
  type ModuleRequestContext,
} from './features/modules/module-registry'
import {
  availableModuleCategories,
  countModulesInCategory,
  moduleActionLabel,
  moduleActionsBlocked,
  moduleCardView,
  selectModuleGroups,
  summariseModuleProgress,
  type ModuleCardViewInput,
  type ModuleFilter,
} from './features/modules/module-card-view'
import { useModuleVerification } from './features/modules/useModuleVerification'
import { COMPATIBILITY_STATUSES, useCompatibilityReports } from './features/compatibility/useCompatibilityReports'

type ViewName = 'library' | 'history'
type CheekyResearchState = 'experimental' | 'prerequisite'

interface CheekyResearchGuide {
  state: CheekyResearchState
  decision: string
  route: string
  prerequisites: string[]
  validation: string[]
  risk: string
}

const installedGames = ref<InstalledGame[]>([])
const transactions = ref<TransactionRecord[]>([])
const gameInspection = ref<GameEnvironmentInspection | null>(null)
const loading = ref(true)
const transactionsLoading = ref(false)
const inspectionLoading = ref(false)
const error = ref<string | null>(null)
const inspectionError = ref<string | null>(null)
const actionError = ref<string | null>(null)
const success = ref<string | null>(null)
const search = ref('')
const supportedOnly = ref(true)
const activeModuleFilter = ref<ModuleFilter>('all')
const selectedAppId = ref<string | null>(null)
const activeView = ref<ViewName>('library')
const moduleBusy = ref(false)
const busyModuleId = ref<string | null>(null)
const rollbackBusyId = ref<string | null>(null)
const SELECTION_DEBOUNCE_MS = 180
const BACKGROUND_TASK_DELAY_MS = 220
const GAME_STATE_POLL_MS = 3000
let gameStatePoll: number | null = null
let selectionDebounce: number | null = null
let selectionBackgroundTimer: number | null = null
let selectionGeneration = 0
let appUnmounted = false
const inspectionRequests = new Map<string, Promise<GameEnvironmentInspection>>()
const obsDialog = ref<{ request: ObsVrRequest; preview: ObsVrPreview } | null>(null)
const optiScalerDialog = ref<{ request: OptiScalerRequest; preview: OptiScalerPreview } | null>(null)
/**
 * Explicit opt-in to replace a third-party proxy DLL. Off by default:
 * Moddin only overwrites an occupant it installed itself, or one the
 * player is knowingly replacing. Toggling it re-runs the preview so the
 * dialog shows the resolution the install will actually use.
 */
const optiAllowReplaceUnknown = ref(false)
const ofxrDialog = ref<{ request: OfxrRequest; preview: OfxrPreview } | null>(null)
const cheekyDialog = ref<{ request: CheekyFoveatedDlssRequest; preview: CheekyFoveatedDlssPreview } | null>(null)
const uevrDialog = ref<{ request: UevrRequest; preview: UevrPreview } | null>(null)
const cheekyGuideDialog = ref<ToolModuleDefinition | null>(null)
/**
 * Set when the user hits Install on a module that declares
 * prerequisites in the catalog. The list is already in install order.
 */
const dependencyDialog = ref<{ module: ToolModuleDefinition; missing: ToolModuleDefinition[] } | null>(null)
const vrLaunchDialog = ref<{ request: VrLaunchRequest; preview: VrLaunchPreview } | null>(null)
const uevrBackendSelections = ref<Record<string, UevrBackend>>({})

const { t, locale } = useI18n()

function messageOf(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

const desktopShortcut = useDesktopShortcut({
  busy: moduleBusy,
  actionError,
  success,
  onCreated: async (module) => {
    await refreshTransactions()
    await moduleVerification.verifyModule(module, true)
  },
})

// AI topbar wiring: the menu emits dumb events; this composable turns
// them into AI dialog opens (audit / diagnose / contribute / etc.).
const aiTopbar = useAiTopbarActions({
  selectedAppId: () => selectedAppId.value,
  selectedGameName: () => selectedGame.value?.catalog?.name ?? null,
  currentError: () => (typeof error.value === 'string' ? error.value : null),
})

/**
 * What the player recorded about a mod, keyed per game. Its own
 * vocabulary, its own dialog and its own storage; the library only asks
 * it what status a module currently has.
 */
const compatibility = useCompatibilityReports({
  t,
  stateKey: (module) => moduleStateKeyFor(module, selectedAppId.value),
  gameLabel: () => selectedGame.value?.catalog?.name ?? selectedGame.value?.installed.name ?? '',
  formatDate: formatTransactionDate,
  success,
})
const {
  dialog: compatibilityDialog,
  draftStatus: compatibilityDraftStatus,
  draftNote: compatibilityDraftNote,
} = compatibility

/**
 * Verifications and update checks for the selected game. Both run in the
 * background after a selection settles, so this owns the generation
 * counter that tells stale work from current work.
 */
const moduleVerification = useModuleVerification({
  t,
  selectedAppId: () => selectedAppId.value,
  selectionGeneration: () => selectionGeneration,
  gameContext: gameContextForAppId,
  requestContext,
  stateKey: moduleStateKeyFor,
  activeTransactionId: (module) => activeModuleTransaction(module)?.id ?? null,
  desktopShortcutPreview: (module) => desktopShortcut.preview(module, selectedGame.value),
  actionError,
  success,
})

function storeLabel(store: InstalledGame['store']) {
  if (store === 'epic') return t('storeEpic')
  if (store === 'gog') return t('storeGog')
  return t('storeSteam')
}

function gameDisplayName(game: InstalledGame) {
  return findCatalogGameByInstalledGame(game)?.name ?? game.name
}

function catalogModCount(game: InstalledGame) {
  return findCatalogGameByInstalledGame(game)?.modules.filter((module) => module.status === 'available').length ?? 0
}

const blockedReason = computed(() => (selectedGameRunning.value ? t('gameRunningBlocked') : undefined))
const dialogBlocked = computed(() => moduleBusy.value || inspectionLoading.value || selectedGameRunning.value)
const activeTransactionCount = computed(() => transactions.value.filter((item) => item.status === 'applied').length)

const supportedInstalledGames = computed(() =>
  installedGames.value.filter((game) => findCatalogGameByInstalledGame(game)),
)

const filteredGames = computed(() => {
  const term = search.value.trim().toLowerCase()
  const ordered = [...installedGames.value].sort((a, b) => {
    const aSupported = Boolean(findCatalogGameByInstalledGame(a))
    const bSupported = Boolean(findCatalogGameByInstalledGame(b))
    if (aSupported !== bSupported) return aSupported ? -1 : 1
    return a.name.localeCompare(b.name)
  })

  return ordered.filter((game) => {
    const matchesSupport = !supportedOnly.value || Boolean(findCatalogGameByInstalledGame(game))
    const matchesSearch = !term || game.name.toLowerCase().includes(term)
    return matchesSupport && matchesSearch
  })
})

const selectedGame = computed(() => {
  const installed = installedGames.value.find((game) => game.appId === selectedAppId.value)
  if (!installed) return null
  return {
    installed,
    catalog: findCatalogGameByInstalledGame(installed),
  }
})

function gameContextForAppId(appId: string | null) {
  if (!appId) return null
  const installed = installedGames.value.find((game) => game.appId === appId)
  if (!installed) return null
  const catalog = findCatalogGameByInstalledGame(installed)
  return catalog ? { installed, catalog } : null
}

const selectedGameRunning = computed(() => {
  if (gameInspection.value?.gameRunning) return true

  const modules = selectedGame.value?.catalog?.modules ?? []
  return modules.some((module) => moduleVerification.verificationFor(module)?.gameRunning === true)
})

/**
 * The OFXR bridge is armed by the launch, not installed by it, so the
 * launch dialog shows what is about to happen. It is the one preview the
 * VR launch request carries along.
 */
const selectedOfxrModule = computed(() =>
  selectedGame.value?.catalog?.modules.find((module) => module.id === 'ofxr-framegen') ?? null,
)

const ofxrLaunchRequest = computed(() => {
  const module = selectedOfxrModule.value
  return module ? buildOfxrRequest(requestContext(module)) : null
})

const ofxrReadyForLaunch = computed(() => {
  const module = selectedOfxrModule.value
  return module ? moduleVerification.verificationFor(module)?.status === 'installed' : true
})

const primaryModule = computed(() => {
  const modules = selectedGame.value?.catalog?.modules ?? []
  return modules.find((module) => module.id === 'vr-launch' && module.status === 'available')
    ?? modules.find((module) => module.status === 'available')
    ?? modules.find((module) => module.id === 'vr-launch')
    ?? null
})

/** Every module the selected game declares, and the grid's share of them. */
const gameModules = computed(() => selectedGame.value?.catalog?.modules ?? [])
const ownedModules = computed(() => libraryOwnedModules(gameModules.value))
const categoryTabs = computed(() => availableModuleCategories(gameModules.value))

const moduleProgress = computed(() =>
  summariseModuleProgress(
    ownedModules.value,
    (module) => moduleCardView(moduleCardInput(module)).state,
  ),
)

const moduleGroups = computed(() =>
  selectModuleGroups(gameModules.value, ownedModules.value, activeModuleFilter.value),
)

/**
 * Everything a module's request builder may read. One place, so a
 * request can never be built against a different game than the one on
 * screen.
 */
function requestContext(module: ToolModuleDefinition): ModuleRequestContext {
  const game = selectedGame.value
  return {
    module,
    // A game the catalog does not know has nothing to build a request
    // from, and the builders used to each re-check that for themselves.
    game: game?.catalog ? { installed: game.installed, catalog: game.catalog } : null,
    t,
    uevrBackend: selectedUevrBackend,
  }
}

/** Where a module's verification, update and report state is remembered. */
function moduleStateKeyFor(module: ToolModuleDefinition, appId: string | null) {
  return moduleStateKey(module.id, appId, gameContextForAppId(appId)?.catalog.id ?? null)
}

/** What the card needs to know about one module, right now. */
function moduleCardInput(module: ToolModuleDefinition): ModuleCardViewInput {
  return {
    module,
    verification: moduleVerification.verificationFor(module),
    verifying: moduleVerification.isVerifying(module),
    loading: inspectionLoading.value,
    busy: moduleBusy.value,
    actionBusy: busyModuleId.value === module.id,
    gameRunning: selectedGameRunning.value,
    blockedReason: blockedReason.value,
    installed: Boolean(activeModuleTransaction(module)),
    update: moduleVerification.updateFor(module),
    updateBusy: moduleVerification.isUpdateBusy(module),
    hasUpdateSource: moduleVerification.hasUpdateSource(module),
    updateSummary: moduleVerification.updateSummary(module),
    compatibility: compatibility.statusFor(module),
    t,
    formatDate: formatTransactionDate,
  }
}

function moduleNameById(id: string, fallback = id) {
  return t(moduleNameKey(id, fallback))
}

function moduleName(module: ToolModuleDefinition) {
  return moduleNameById(module.id, module.name)
}

function moduleDescription(module: ToolModuleDefinition) {
  return t(moduleDescriptionKey(module))
}

function gameNameById(gameId: string) {
  return gameCatalog.find((game) => game.id === gameId)?.name ?? gameId
}

function transactionBlockedReason(transaction: TransactionRecord) {
  return selectedGameRunning.value && transaction.gameId === selectedGame.value?.catalog?.id
    ? t('gameRunningBlocked')
    : undefined
}

function categoryLabel(category: ToolModuleDefinition['category']) {
  return t(`category${category.charAt(0).toUpperCase()}${category.slice(1)}`)
}

/**
 * The applied transaction for this module, if one is still on record. A
 * card with one offers "Remove", and only a card with one can be
 * removed.
 */
function activeModuleTransaction(module: ToolModuleDefinition) {
  const gameId = selectedGame.value?.catalog?.id
  const kind = moduleTransactionKind(module.id)
  if (!gameId || !kind) return null
  return transactions.value.find((transaction) =>
    transaction.status === 'applied' && transaction.gameId === gameId && transaction.kind === kind,
  ) ?? null
}

function cheekyResearchStateLabel(state: CheekyResearchState) {
  return state === 'prerequisite' ? t('cheekyGuidePrerequisite') : t('cheekyGuideExperimental')
}

function cheekyResearchGuide(module: ToolModuleDefinition): CheekyResearchGuide {
  const gameId = selectedGame.value?.catalog?.id
  const isEldenRing = gameId === 'elden-ring'
  const isStalker2 = gameId === 'stalker-2'

  return {
    state: isEldenRing ? 'prerequisite' : 'experimental',
    decision: t(
      isEldenRing
        ? 'cheekyGuideEldenDecision'
        : isStalker2
          ? 'cheekyGuideStalker2Decision'
          : 'cheekyGuideCyberpunkDecision',
    ),
    route: t(
      isEldenRing
        ? 'cheekyGuideEldenRoute'
        : isStalker2
          ? 'cheekyGuideStalker2Route'
          : 'cheekyGuideCyberpunkRoute',
    ),
    prerequisites: [
      ...(isEldenRing ? [t('cheekyGuideEldenProviderPrerequisite')] : []),
      t('cheekyGuideReShadePrerequisite'),
      t('cheekyGuideOneIntegration'),
      t('cheekyGuideOpenXrPrerequisite'),
    ],
    validation: [
      t('cheekyGuideTestEnableDlss'),
      t('cheekyGuideTestCompare'),
      t('cheekyGuideTestRecord'),
    ],
    risk: t('cheekyGuideRisksSummary'),
  }
}

function compatibilityStatusSummary(module: ToolModuleDefinition) {
  const report = compatibility.reportFor(module)
  if (report) {
    const recordedAt = t('compatibilityRecordedAt', {
      date: formatTransactionDate(report.updatedAt),
      version: report.testedVersion,
    })
    return report.note ? `${report.note} · ${recordedAt}` : recordedAt
  }

  if (cheekyResearchGuide(module).state === 'prerequisite') {
    return t('cheekyGuideEldenDecision')
  }

  return `${t('compatibilityUpstreamEvidence')} ${cheekyCompatibilityNote(module, selectedGame.value?.catalog?.id, t)}`
}

function openCompatibilityReport(module: ToolModuleDefinition) {
  cheekyGuideDialog.value = null
  compatibility.openRecord(module)
}

async function openRelease(url: string | null) {
  if (!url) return

  try {
    await invoke('open_external_url', { url })
  } catch (err) {
    // Keep the link useful when the UI is running directly in a browser.
    const opened = window.open(url, '_blank', 'noopener,noreferrer')
    if (!opened) actionError.value = messageOf(err)
  }
}

/**
 * Catalog modules the selected game already has installed. A module
 * counts as installed when its last verification says so, or when an
 * applied transaction is still on record for it.
 */
function installedCatalogModuleIds(): Set<string> {
  const game = selectedGame.value?.catalog
  if (!game) return new Set()
  return new Set(
    game.modules
      .filter((module) => moduleVerification.verificationFor(module)?.status === 'installed' || activeModuleTransaction(module))
      .map((module) => module.id),
  )
}

/**
 * Prerequisites a module declares in the catalog (`dependencies`), in
 * the order they must be installed and skipping the ones already
 * present. Returns the full module definitions so the UI can name them
 * and run their own preview flow.
 */
function missingDependenciesFor(module: ToolModuleDefinition): ToolModuleDefinition[] {
  const game = selectedGame.value?.catalog
  if (!game) return []
  return getMissingDependenciesForModule(game, module.id, installedCatalogModuleIds())
    .map((id) => game.modules.find((item) => item.id === id))
    .filter((item): item is ToolModuleDefinition => Boolean(item))
}

async function configureModule(module: ToolModuleDefinition) {
  actionError.value = null
  success.value = null

  if (selectedGameRunning.value) {
    actionError.value = t('gameRunningActionBlocked')
    return
  }

  // A module built on top of another (Cheeky on UEVR, Cheeky on
  // UEVR + OptiScaler) is pointless without it, so offer the
  // prerequisites instead of applying something that cannot work.
  const missing = missingDependenciesFor(module)
  if (missing.length) {
    dependencyDialog.value = { module, missing }
    return
  }

  busyModuleId.value = module.id
  try {
    await openModulePreview(module)
  } finally {
    busyModuleId.value = null
  }
}

/** Start the normal preview flow for a prerequisite from the dialog. */
async function installDependency(dependency: ToolModuleDefinition) {
  dependencyDialog.value = null
  await configureModule(dependency)
}

/**
 * Open the change preview for a module.
 *
 * The desktop shortcut owns its own dialog, so it goes first and says so
 * by handling the card. Everything else asks the registry which command
 * answers for this id, and the module's own row says which dialog the
 * answer belongs in.
 */
async function openModulePreview(module: ToolModuleDefinition) {
  if (await desktopShortcut.open(module, selectedGame.value)) return

  const entry = libraryModuleEntry(module.id)
  moduleBusy.value = true
  try {
    if (!entry) throw new Error(t('moduleNoAction', { module: module.id }))
    // A fresh OptiScaler preview must not inherit the previous
    // "replace anyway".
    if (entry.dialog === 'optiscaler') optiAllowReplaceUnknown.value = false

    const opened = await entry.openPreview(requestContext(module))
    switch (opened.dialog) {
      case 'vr-launch': vrLaunchDialog.value = opened; break
      case 'obs-vr': obsDialog.value = opened; break
      case 'optiscaler': optiScalerDialog.value = opened; break
      case 'ofxr': ofxrDialog.value = opened; break
      case 'cheeky': cheekyDialog.value = opened; break
      case 'uevr': uevrDialog.value = opened; break
    }
  } catch (err) {
    actionError.value = messageOf(err)
  } finally {
    moduleBusy.value = false
  }
}

async function launchVrGame() {
  if (!vrLaunchDialog.value) return

  if (selectedGameRunning.value || vrLaunchDialog.value.preview.gameRunning) {
    actionError.value = t('gameRunningActionBlocked')
    return
  }

  actionError.value = null
  success.value = null
  moduleBusy.value = true
  try {
    const request = vrLaunchDialog.value.request
    const result = await launchVrGameCommand(request, ofxrLaunchRequest.value)
    vrLaunchDialog.value = null
    success.value = t('vrLaunchSuccess', { game: request.gameName })
    if (result.transaction) success.value += ` ${t('vrConfigSaved')}`
    await refreshTransactions()
    await moduleVerification.verifyAvailableModules()
  } catch (err) {
    actionError.value = messageOf(err)
  } finally {
    moduleBusy.value = false
  }
}

async function applyObsConfiguration() {
  if (!obsDialog.value) return

  if (selectedGameRunning.value || obsDialog.value.preview.gameRunning) {
    actionError.value = t('gameRunningActionBlocked')
    return
  }

  actionError.value = null
  success.value = null
  moduleBusy.value = true
  try {
    const configuredSource = obsDialog.value.request.sourceName
    await configureObsVr(obsDialog.value.request)
    obsDialog.value = null
    success.value = t('installSuccess', { name: configuredSource })
    await refreshTransactions()
    await moduleVerification.verifyAvailableModules()
  } catch (err) {
    actionError.value = messageOf(err)
  } finally {
    moduleBusy.value = false
  }
}

/**
 * Re-run the OptiScaler preview after the player ticks "replace anyway",
 * and pin the resulting resolution on the request. The install refuses
 * a request whose pinned resolution no longer matches the environment,
 * so a preview that went stale between opening and applying fails loudly
 * instead of overwriting whatever took the slot in the meantime.
 */
async function setOptiReplaceUnknown(allow: boolean) {
  if (!optiScalerDialog.value) return
  optiAllowReplaceUnknown.value = allow
  moduleBusy.value = true
  try {
    const request: OptiScalerRequest = { ...optiScalerDialog.value.request, allowReplaceUnknown: allow }
    const preview = await previewOptiScaler(request)
    optiScalerDialog.value = { request: { ...request, resolution: preview.resolution }, preview }
  } catch (err) {
    actionError.value = messageOf(err)
  } finally {
    moduleBusy.value = false
  }
}

async function applyOptiScaler() {
  if (!optiScalerDialog.value) return

  if (selectedGameRunning.value || optiScalerDialog.value.preview.gameRunning) {
    actionError.value = t('gameRunningActionBlocked')
    return
  }

  actionError.value = null
  success.value = null
  moduleBusy.value = true
  try {
    const request: OptiScalerRequest = {
      ...optiScalerDialog.value.request,
      allowReplaceUnknown: optiAllowReplaceUnknown.value,
      resolution: optiScalerDialog.value.preview.resolution,
    }
    await installOptiScaler(request)
    optiScalerDialog.value = null
    success.value = t('installSuccess', { name: `OptiScaler ${request.version}` })
    await refreshTransactions()
    await inspectSelectedGame()
    await moduleVerification.verifyAvailableModules()
  } catch (err) {
    actionError.value = messageOf(err)
  } finally {
    moduleBusy.value = false
  }
}

async function applyOfxr() {
  if (!ofxrDialog.value) return

  if (selectedGameRunning.value || ofxrDialog.value.preview.gameRunning) {
    actionError.value = t('gameRunningActionBlocked')
    return
  }

  actionError.value = null
  success.value = null
  moduleBusy.value = true
  try {
    const request = ofxrDialog.value.request
    const result = await installOfxr(request)
    if (!result.armed) throw new Error(t('ofxrActivationFailed'))
    ofxrDialog.value = null
    success.value = result.transaction
      ? t('installSuccess', { name: moduleNameById('ofxr-framegen') })
      : t('ofxrReadyForLaunch')
    await refreshTransactions()
    await moduleVerification.verifyAvailableModules()
  } catch (err) {
    actionError.value = messageOf(err)
  } finally {
    moduleBusy.value = false
  }
}

async function applyCheeky() {
  if (!cheekyDialog.value) return

  if (selectedGameRunning.value || cheekyDialog.value.preview.gameRunning) {
    actionError.value = t('gameRunningActionBlocked')
    return
  }

  actionError.value = null
  success.value = null
  moduleBusy.value = true
  try {
    await installCheekyFoveatedDlss(cheekyDialog.value.request)
    cheekyDialog.value = null
    success.value = t('installSuccess', { name: moduleNameById('cheeky-foveated-dlss') })
    await refreshTransactions()
    await moduleVerification.verifyAvailableModules()
  } catch (err) {
    actionError.value = messageOf(err)
  } finally {
    moduleBusy.value = false
  }
}

async function applyUevr() {
  if (!uevrDialog.value) return

  if (selectedGameRunning.value || uevrDialog.value.preview.gameRunning || uevrDialog.value.preview.uevrRunning) {
    actionError.value = t('gameRunningActionBlocked')
    return
  }

  actionError.value = null
  success.value = null
  moduleBusy.value = true
  try {
    const request = uevrDialog.value.request
    const result = await installUevr(request)
    uevrDialog.value = null
    success.value = t('installSuccess', { name: `${uevrBackendLabel(request.backend)} ${result.metadata?.version ?? ''}`.trim() })
    await refreshTransactions()
    await moduleVerification.verifyAvailableModules()
  } catch (err) {
    actionError.value = messageOf(err)
  } finally {
    moduleBusy.value = false
  }
}

/**
 * Undo a module. Most have a command of their own; the rest are undone by
 * rolling their last transaction back, which is the whole of what they
 * recorded.
 */
async function removeModule(module: ToolModuleDefinition) {
  if (selectedGameRunning.value) {
    actionError.value = t('gameRunningActionBlocked')
    return
  }

  const gameId = selectedGame.value?.catalog?.id
  const kind = moduleTransactionKind(module.id)
  if (!gameId || !kind || !activeModuleTransaction(module)) {
    actionError.value = t('noModuleTransaction')
    return
  }

  actionError.value = null
  success.value = null
  moduleBusy.value = true
  try {
    const entry = libraryModuleEntry(module.id)
    if (entry?.remove) await entry.remove(requestContext(module))
    else await rollbackLatestModuleTransaction(gameId, kind)

    success.value = t('moduleRemoved', { module: moduleName(module) })
    await refreshTransactions()
    await inspectSelectedGame()
    await moduleVerification.verifyModule(module, true)
  } catch (err) {
    actionError.value = messageOf(err)
  } finally {
    moduleBusy.value = false
  }
}

async function rollback(transaction: TransactionRecord) {
  if (selectedGameRunning.value && transaction.gameId === selectedGame.value?.catalog?.id) {
    actionError.value = t('gameRunningActionBlocked')
    return
  }

  actionError.value = null
  success.value = null
  rollbackBusyId.value = transaction.id
  try {
    await invoke<TransactionRecord>('rollback_transaction', { id: transaction.id })
    success.value = t('rolledBack', { label: transaction.label })
    await refreshTransactions()
    await inspectSelectedGame()
    await moduleVerification.verifyAvailableModules()
  } catch (err) {
    actionError.value = messageOf(err)
  } finally {
    rollbackBusyId.value = null
  }
}

function formatTransactionDate(timestamp: number) {
  return new Intl.DateTimeFormat(locale.value, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(new Date(timestamp))
}

function formatFileSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

interface UevrBackendOption {
  id: UevrBackend
  compatibility: UevrBackendCompatibility
}

function isUevrBackend(value: string): value is UevrBackend {
  return ['nightly', 'joey', 'afw', 'joey-afw'].includes(value)
}

function isUevrBackendCompatibility(value: string): value is UevrBackendCompatibility {
  return ['preferred', 'available', 'experimental', 'unknown', 'not_working'].includes(value)
}

function uevrBackendOptions(module: ToolModuleDefinition): UevrBackendOption[] {
  const configured = configList(module.config, 'backendCandidates')
  const parsed = configured.flatMap((value) => {
    const [backend, compatibility = 'unknown'] = value.split('|').map((item) => item.trim())
    if (!backend || !isUevrBackend(backend)) return []
    return [{
      id: backend,
      compatibility: isUevrBackendCompatibility(compatibility) ? compatibility : 'unknown',
    }]
  })
  if (parsed.length) return parsed
  return [{ id: 'nightly', compatibility: 'unknown' }]
}

function uevrBackendLabel(backend: UevrBackend) {
  if (backend === 'joey') return t('uevrBackendJoey')
  if (backend === 'afw') return t('uevrBackendAfw')
  if (backend === 'joey-afw') return t('uevrBackendJoeyAfw')
  return t('uevrBackendNightly')
}

function uevrBackendCompatibilityLabel(status: UevrBackendCompatibility) {
  if (status === 'preferred') return t('uevrCompatibilityPreferred')
  if (status === 'available') return t('uevrCompatibilityAvailable')
  if (status === 'experimental') return t('uevrCompatibilityExperimental')
  if (status === 'not_working') return t('uevrCompatibilityNotWorking')
  return t('uevrCompatibilityUnknown')
}

function selectedUevrBackend(module: ToolModuleDefinition): UevrBackend {
  const options = uevrBackendOptions(module)
  const usableOptions = options.filter((option) => option.compatibility !== 'not_working')
  const selected = uevrBackendSelections.value[moduleStateKeyFor(module, selectedAppId.value)]
  if (selected && usableOptions.some((option) => option.id === selected)) return selected
  const preferred = usableOptions.find((option) => option.compatibility === 'preferred')
  if (preferred) return preferred.id
  const configured = module.config?.defaultBackend
  if (typeof configured === 'string' && isUevrBackend(configured) && usableOptions.some((option) => option.id === configured)) {
    return configured
  }
  return (usableOptions[0] ?? options[0]).id
}

function setUevrBackend(module: ToolModuleDefinition, value: string) {
  if (!isUevrBackend(value) || !uevrBackendOptions(module).some((option) => option.id === value && option.compatibility !== 'not_working')) return
  uevrBackendSelections.value = { ...uevrBackendSelections.value, [moduleStateKeyFor(module, selectedAppId.value)]: value }
  void moduleVerification.verifyModule(module, true)
}

function onUevrBackendChange(module: ToolModuleDefinition, event: Event) {
  const target = event.target
  if (target instanceof HTMLSelectElement) setUevrBackend(module, target.value)
}

async function refreshGames() {
  loading.value = true
  error.value = null
  try {
    installedGames.value = await invoke<InstalledGame[]>('detect_installed_games')
    if (!supportedInstalledGames.value.length) supportedOnly.value = false
    if (!selectedAppId.value || !installedGames.value.some((game) => game.appId === selectedAppId.value)) {
      selectedAppId.value = supportedInstalledGames.value[0]?.appId ?? installedGames.value[0]?.appId ?? null
    } else {
      await inspectSelectedGame()
    }
  } catch (err) {
    error.value = messageOf(err)
  } finally {
    loading.value = false
  }
}

async function inspectSelectedGame(
  silent = false,
  expectedAppId = selectedAppId.value,
  expectedGeneration = selectionGeneration,
) {
  const game = gameContextForAppId(expectedAppId)
  if (!game) {
    if (!silent && selectedAppId.value === expectedAppId) inspectionLoading.value = false
    return
  }

  if (!silent && selectedAppId.value === expectedAppId && selectionGeneration === expectedGeneration) {
    inspectionLoading.value = true
    inspectionError.value = null
  }

  const requestKey = `${expectedAppId}:${game.installed.installDir}:${game.catalog.executable}`
  let request = inspectionRequests.get(requestKey)
  if (!request) {
    request = invoke<GameEnvironmentInspection>('inspect_game_environment', {
      installDir: game.installed.installDir,
      executable: game.catalog.executable,
    }).finally(() => {
      if (inspectionRequests.get(requestKey) === request) inspectionRequests.delete(requestKey)
    })
    inspectionRequests.set(requestKey, request)
  }

  const previousGameRunning = gameInspection.value?.gameRunning
  try {
    const nextInspection = await request
    if (selectedAppId.value !== expectedAppId || selectionGeneration !== expectedGeneration) return
    gameInspection.value = nextInspection
    if (silent && previousGameRunning !== undefined && previousGameRunning !== nextInspection.gameRunning) {
      void moduleVerification.verifyAvailableModules(expectedAppId, expectedGeneration, true)
    }
  } catch (err) {
    if (!silent && selectedAppId.value === expectedAppId && selectionGeneration === expectedGeneration) {
      inspectionError.value = messageOf(err)
    }
  } finally {
    if (!silent && selectedAppId.value === expectedAppId && selectionGeneration === expectedGeneration) {
      inspectionLoading.value = false
    }
  }
}

async function refreshTransactions() {
  transactionsLoading.value = true
  try {
    transactions.value = await invoke<TransactionRecord[]>('list_transactions')
  } catch (err) {
    actionError.value = messageOf(err)
  } finally {
    transactionsLoading.value = false
  }
}

async function switchView(view: ViewName) {
  activeView.value = view
  actionError.value = null
  success.value = null
  if (view === 'history') await refreshTransactions()
}

function clearSelectionWorkTimers() {
  if (selectionDebounce !== null) window.clearTimeout(selectionDebounce)
  if (selectionBackgroundTimer !== null) window.clearTimeout(selectionBackgroundTimer)
  selectionDebounce = null
  selectionBackgroundTimer = null
}

function scheduleSelectedGameWork() {
  selectionGeneration += 1
  const expectedGeneration = selectionGeneration
  const expectedAppId = selectedAppId.value
  clearSelectionWorkTimers()
  gameInspection.value = null
  inspectionError.value = null

  const game = gameContextForAppId(expectedAppId)
  inspectionLoading.value = Boolean(game)
  if (!game) return

  selectionDebounce = window.setTimeout(async () => {
    selectionDebounce = null
    await inspectSelectedGame(false, expectedAppId, expectedGeneration)
    if (selectedAppId.value !== expectedAppId || selectionGeneration !== expectedGeneration) return

    selectionBackgroundTimer = window.setTimeout(() => {
      selectionBackgroundTimer = null
      void moduleVerification.verifyAvailableModules(expectedAppId, expectedGeneration)
      void moduleVerification.checkAvailableModuleUpdates(expectedAppId, expectedGeneration)
    }, BACKGROUND_TASK_DELAY_MS)
  }, SELECTION_DEBOUNCE_MS)
}

function scheduleGameStatePoll() {
  if (appUnmounted) return
  gameStatePoll = window.setTimeout(async () => {
    gameStatePoll = null
    const expectedAppId = selectedAppId.value
    const expectedGeneration = selectionGeneration
    // Skip the process check while the window is minimised or hidden: nobody
    // is looking, and each check spawns a helper process on Windows.
    const visible = typeof document === 'undefined' || document.visibilityState === 'visible'
    if (visible && activeView.value === 'library' && gameContextForAppId(expectedAppId)) {
      await inspectSelectedGame(true, expectedAppId, expectedGeneration)
    }
    scheduleGameStatePoll()
  }, GAME_STATE_POLL_MS)
}

watch(selectedAppId, () => {
  activeModuleFilter.value = 'all'
  scheduleSelectedGameWork()
})

watch(locale, (value) => {
  try {
    window.localStorage.setItem('moddin-locale', value)
  } catch {
    // Ignore unavailable storage, such as privacy-restricted browser contexts.
  }
})

onMounted(async () => {
  appUnmounted = false
  compatibility.load()
  await Promise.all([refreshGames(), refreshTransactions()])
  scheduleGameStatePoll()
})

onUnmounted(() => {
  appUnmounted = true
  if (gameStatePoll !== null) window.clearTimeout(gameStatePoll)
  clearSelectionWorkTimers()
})
</script>

<template>
  <div class="app-shell" :lang="locale">
    <aside class="sidebar">
      <div class="brand">
        <div class="brand-mark" aria-hidden="true">M</div>
        <div>
          <strong>Moddin</strong>
          <small>{{ t('brandTagline') }}</small>
        </div>
      </div>

      <nav class="nav-section" :aria-label="t('navMain')">
        <button
          class="nav-item"
          type="button"
          :aria-current="activeView === 'library' ? 'page' : undefined"
          @click="switchView('library')"
        >
          <AppIcon name="library" />
          <span>{{ t('navLibrary') }}</span>
          <span v-if="supportedInstalledGames.length" class="nav-count">{{ supportedInstalledGames.length }}</span>
        </button>
        <button
          class="nav-item"
          type="button"
          :aria-current="activeView === 'history' ? 'page' : undefined"
          @click="switchView('history')"
        >
          <AppIcon name="history" />
          <span>{{ t('navHistory') }}</span>
          <span v-if="activeTransactionCount" class="nav-count">{{ activeTransactionCount }}</span>
        </button>
      </nav>

      <GlobalTools />

      <footer class="sidebar-footer">
        <label>
          <span class="sr-only">{{ t('language') }}</span>
          <select v-model="locale" class="select">
            <option v-for="option in localeOptions" :key="option.value" :value="option.value">{{ option.label }}</option>
          </select>
        </label>
        <small>{{ t('appVersion', { version: appVersion, count: gameCatalog.length }) }}</small>
      </footer>
    </aside>

    <main class="main-content">
      <section v-if="activeView === 'library'" class="page">
        <div v-if="error" class="callout callout-danger" role="alert">
          <AppIcon class="callout-icon" name="alert" />
          <div class="callout-body">
            <strong>{{ t('libraryScanFailed') }}</strong>
            <p>{{ error }}</p>
          </div>
          <button
            class="btn btn-sm callout-action"
            type="button"
            @click="aiTopbar.diagnose"
          >
            {{ t('aiExplainError') }}
          </button>
        </div>

        <div class="library-layout">
          <GameList
            v-model:search="search"
            v-model:supported-only="supportedOnly"
            :games="filteredGames"
            :selected-app-id="selectedAppId"
            :loading="loading"
            :total-count="installedGames.length"
            :supported-count="supportedInstalledGames.length"
            :mod-count="catalogModCount"
            :display-name="gameDisplayName"
            @select="selectedAppId = $event"
            @rescan="refreshGames"
          />

          <div class="panel game-detail">
            <EmptyState
              v-if="!selectedGame"
              class="game-detail-empty"
              icon="gamepad"
              :title="t('selectGameTitle')"
              :description="t('selectGameHint')"
            />

            <template v-else>
              <header class="game-hero">
                <div class="game-hero-copy">
                  <p class="overline">{{ storeLabel(selectedGame.installed.store) }}</p>
                  <h1>{{ selectedGame.catalog?.name ?? selectedGame.installed.name }}</h1>
                  <div v-if="selectedGame.catalog && moduleProgress.total" class="game-progress">
                    <div class="progress-track" aria-hidden="true">
                      <span :style="{ width: `${(moduleProgress.active / moduleProgress.total) * 100}%` }" />
                    </div>
                    <span>{{ t('progressSummary', { active: moduleProgress.active, total: moduleProgress.total }) }}</span>
                    <span v-if="moduleProgress.attention" class="badge badge-warning">
                      {{ t('progressAttention', { count: moduleProgress.attention }, moduleProgress.attention) }}
                    </span>
                    <span v-else-if="moduleProgress.checking" class="badge is-loading">{{ t('progressChecking') }}</span>
                  </div>
                </div>
                <div class="game-hero-actions">
                  <AiTopbarMenu @ask="aiTopbar.ask" />
                  <button
                    v-if="primaryModule?.id === 'vr-launch'"
                    class="btn btn-primary btn-lg"
                    :class="{ 'is-loading': busyModuleId === primaryModule.id }"
                    type="button"
                    :disabled="moduleActionsBlocked(moduleCardInput(primaryModule))"
                    :title="blockedReason"
                    @click="configureModule(primaryModule)"
                  >
                    <AppIcon v-if="busyModuleId !== primaryModule.id" name="play" />
                    {{ moduleActionLabel(moduleCardInput(primaryModule)) }}
                  </button>
                </div>
              </header>

              <div v-if="selectedGameRunning" class="callout callout-warning" role="status">
                <AppIcon class="callout-icon" name="info" />
                <div>
                  <strong>{{ t('gameRunningTitle') }}</strong>
                  <p>{{ t('gameRunningHint') }}</p>
                </div>
              </div>

              <AiGameSuggestions
                v-if="selectedGame.catalog"
                :game-id="selectedGame.catalog.id"
                :game-name="selectedGame.catalog.name"
              />

              <template v-if="selectedGame.catalog">
                <section class="mods-section">
                  <div class="section-heading">
                    <div>
                      <h2>{{ t('modsTitle') }}</h2>
                      <p>{{ t('modsSubtitle') }}</p>
                    </div>
                    <div v-if="categoryTabs.length > 1" class="segmented" role="group" :aria-label="t('modsTitle')">
                      <button type="button" :aria-pressed="activeModuleFilter === 'all'" @click="activeModuleFilter = 'all'">
                        {{ t('filterAllMods') }}
                      </button>
                      <button
                        v-for="category in categoryTabs"
                        :key="category"
                        type="button"
                        :aria-pressed="activeModuleFilter === category"
                        @click="activeModuleFilter = category"
                      >
                        {{ categoryLabel(category) }} <span class="count">{{ countModulesInCategory(gameModules, category) }}</span>
                      </button>
                    </div>
                  </div>

                  <section v-for="group in moduleGroups" :key="group.category" class="mod-group">
                    <h3 class="mod-group-title">{{ categoryLabel(group.category) }}</h3>
                    <div class="mod-grid">
                      <ModuleCard
                        v-for="module in group.modules"
                        :key="module.id"
                        v-bind="moduleCardView(moduleCardInput(module))"
                        @action="configureModule(module)"
                        @verify="moduleVerification.verifyModule(module)"
                        @remove="removeModule(module)"
                        @check-update="moduleVerification.checkModuleUpdate(module)"
                        @open-release="openRelease(moduleVerification.updateFor(module)?.releaseUrl ?? null)"
                      >
                        <template v-if="module.id === 'cheeky-foveated-dlss'" #details>
                          <section class="detail-block">
                            <h5 class="detail-title">{{ t('compatibilityStatus') }}</h5>
                            <p class="detail-copy">{{ cheekyResearchGuide(module).decision }}</p>
                            <p v-if="compatibility.reportFor(module)" class="detail-copy text-faint">{{ compatibilityStatusSummary(module) }}</p>
                            <div class="detail-row">
                              <button class="btn btn-sm" type="button" @click="cheekyGuideDialog = module">{{ t('viewCompatibilityGuide') }}</button>
                              <button class="btn btn-ghost btn-sm" type="button" @click="openCompatibilityReport(module)">{{ t('recordTest') }}</button>
                            </div>
                          </section>
                        </template>
                        <template v-else-if="module.id === 'uevr'" #details>
                          <section class="detail-block">
                            <div class="detail-row spread">
                              <h5 class="detail-title">{{ t('uevrEngineLabel') }}</h5>
                              <span v-if="inspectionLoading && !gameInspection" class="badge is-loading">{{ t('inspecting') }}</span>
                              <span v-else class="badge" :class="gameInspection?.engine === 'Unreal Engine' ? 'badge-success' : 'badge-warning'">
                                {{ gameInspection?.engine === 'Unreal Engine' ? t('uevrEngineEligible') : t('uevrEngineBlocked') }}
                              </span>
                            </div>
                            <p class="detail-copy">
                              {{ gameInspection?.engine ?? t('unknownEngine') }}<template v-if="gameInspection?.engineVersion"> · {{ gameInspection.engineVersion }}</template>
                            </p>
                            <label class="field">
                              <span>{{ t('uevrBackend') }}</span>
                              <select
                                class="select"
                                :value="selectedUevrBackend(module)"
                                :disabled="moduleBusy || selectedGameRunning"
                                @change="onUevrBackendChange(module, $event)"
                              >
                                <option
                                  v-for="option in uevrBackendOptions(module)"
                                  :key="option.id"
                                  :value="option.id"
                                  :disabled="option.compatibility === 'not_working'"
                                >
                                  {{ uevrBackendLabel(option.id) }} ({{ uevrBackendCompatibilityLabel(option.compatibility) }})
                                </option>
                              </select>
                              <small>{{ t('uevrCompatibilityHint') }}</small>
                            </label>
                          </section>
                        </template>
                      </ModuleCard>
                    </div>
                  </section>
                </section>

                <CapabilityModulesSection
                  :game-id="selectedGame.catalog.id"
                  :game-name="selectedGame.catalog.name"
                  :install-dir="selectedGame.installed.installDir"
                  :executable-dir="gameInspection?.executableDirectory ?? null"
                  :engine="selectedGame.catalog.enginePreset ?? null"
                  :exclude-ids="ownedModules.map((module) => module.id)"
                  :transactions="transactions"
                  @refresh-request="refreshTransactions"
                />

                <details class="disclosure advanced-panel">
                  <summary>
                    <span>{{ t('advancedTitle') }}</span>
                    <small>{{ t('advancedHint') }}</small>
                  </summary>
                  <div class="advanced-body">
                    <dl class="info-list">
                      <div>
                        <dt>{{ t('advancedInstallFolder') }}</dt>
                        <dd class="path-text">{{ selectedGame.installed.installDir }}</dd>
                      </div>
                      <div>
                        <dt>{{ t('advancedExecutable') }}</dt>
                        <dd class="path-text">{{ gameInspection?.executablePath ?? selectedGame.catalog.executable }}</dd>
                      </div>
                      <div>
                        <dt>{{ t('advancedStoreId') }}</dt>
                        <dd>{{ storeLabel(selectedGame.installed.store) }} · {{ selectedGame.installed.appId }}</dd>
                      </div>
                      <div v-if="gameInspection">
                        <dt>{{ t('advancedEngine') }}</dt>
                        <dd>
                          {{ gameInspection.engine ?? t('unknownEngine') }}<template v-if="gameInspection.engineVersion"> · {{ gameInspection.engineVersion }}</template>
                        </dd>
                      </div>
                      <div v-if="gameInspection">
                        <dt>{{ t('advancedModFiles') }}</dt>
                        <dd>
                          <template v-if="gameInspection.proxyDlls.length">
                            <span v-for="dll in gameInspection.proxyDlls" :key="dll.path" class="dll-chip" :title="dll.path">
                              {{ dll.name }} · {{ formatFileSize(dll.sizeBytes) }}
                            </span>
                          </template>
                          <span v-else class="text-muted">{{ t('advancedNoModFiles') }}</span>
                        </dd>
                      </div>
                    </dl>
                    <p v-if="inspectionError" class="callout callout-danger">{{ inspectionError }}</p>
                    <div>
                      <button class="btn btn-sm" :class="{ 'is-loading': inspectionLoading }" type="button" :disabled="inspectionLoading" @click="inspectSelectedGame()">
                        {{ inspectionLoading ? t('inspecting') : t('advancedRescan') }}
                      </button>
                    </div>
                  </div>
                </details>
              </template>

              <EmptyState
                v-else
                icon="info"
                :title="t('notCataloguedTitle')"
                :description="t('notCataloguedHint')"
              />
            </template>
          </div>
        </div>
      </section>

      <HistoryView
        v-else
        :transactions="transactions"
        :loading="transactionsLoading"
        :busy-id="rollbackBusyId"
        :game-name="gameNameById"
        :kind-label="moduleNameById"
        :format-date="formatTransactionDate"
        :blocked-reason="transactionBlockedReason"
        @refresh="refreshTransactions"
        @undo="rollback"
      />
    </main>

    <ToastStack
      :success="success"
      :error="actionError"
      @dismiss-success="success = null"
      @dismiss-error="actionError = null"
    />

    <BaseDialog
      v-if="obsDialog"
      :eyebrow="t('previewEyebrow')"
      :title="moduleNameById('obs-vr')"
      :description="obsDialog.request.gameName"
      @close="obsDialog = null"
    >
      <div class="fact-grid">
        <div class="fact"><span>{{ t('obsCollection') }}</span><strong>{{ obsDialog.preview.collectionName ?? t('notFound') }}</strong></div>
        <div class="fact"><span>{{ t('obsScene') }}</span><strong>{{ obsDialog.request.sceneName }}</strong></div>
        <div class="fact"><span>{{ t('obsSource') }}</span><strong>{{ obsDialog.request.sourceName }}</strong></div>
      </div>
      <ChangePreview :changes="obsDialog.preview.changes" :warnings="obsDialog.preview.warnings" :location="obsDialog.preview.collectionFile" />
      <template #footer>
        <span v-if="blockedReason" class="footer-hint">{{ blockedReason }}</span>
        <button class="btn" type="button" @click="obsDialog = null">{{ t('cancel') }}</button>
        <button class="btn btn-primary" :class="{ 'is-loading': moduleBusy }" type="button" :disabled="!obsDialog.preview.canApply || dialogBlocked" @click="applyObsConfiguration">
          {{ moduleBusy ? t('previewWorking') : t('previewApply') }}
        </button>
      </template>
    </BaseDialog>

    <BaseDialog
      v-if="optiScalerDialog"
      :eyebrow="t('previewEyebrow')"
      :title="moduleNameById('optiscaler')"
      :description="optiScalerDialog.request.gameName"
      @close="optiScalerDialog = null"
    >
      <div class="fact-grid">
        <div class="fact"><span>{{ t('factVersion') }}</span><strong>{{ optiScalerDialog.request.version }}</strong></div>
        <div class="fact"><span>{{ t('factLoadedVia') }}</span><strong>{{ optiScalerDialog.preview.selectedProxy ?? t('notFound') }}</strong></div>
        <div class="fact">
          <span>{{ t('factStatus') }}</span>
          <strong>{{ optiScalerDialog.preview.installed ? t('factInstalled', { version: optiScalerDialog.preview.installedVersion ?? '?' }) : t('factNotInstalled') }}</strong>
        </div>
      </div>
      <ChangePreview :changes="optiScalerDialog.preview.changes" :warnings="optiScalerDialog.preview.warnings" :location="optiScalerDialog.preview.executableDirectory">
        <section v-if="optiScalerDialog.preview.conflicts.length" class="dialog-section">
          <h3>{{ t('optiConflictsTitle') }}</h3>
          <p>{{ t('optiConflictsHint') }}</p>
          <ul class="note-list">
            <li v-for="conflict in optiScalerDialog.preview.conflicts" :key="conflict.path">
              {{ conflict.name }} · {{ formatFileSize(conflict.sizeBytes) }}
              <span class="dll-chip">
                {{ conflict.managedByModdin ? t('optiConflictManaged') : t('optiConflictForeign', { holder: conflict.heldBy || t('unknownHolder') }) }}
              </span>
            </li>
          </ul>
          <label v-if="optiScalerDialog.preview.resolution === 'blocked'" class="field opti-replace">
            <span>
              <input
                type="checkbox"
                :checked="optiAllowReplaceUnknown"
                :disabled="moduleBusy"
                @change="setOptiReplaceUnknown(($event.target as HTMLInputElement).checked)"
              />
              {{ t('optiReplaceUnknown') }}
            </span>
            <small>{{ t('optiReplaceUnknownHint') }}</small>
          </label>
          <p v-else-if="optiScalerDialog.preview.resolution === 'replace_with_backup'" class="detail-text">
            {{ t('optiReplaceWithBackup') }}
          </p>
        </section>
      </ChangePreview>
      <template #footer>
        <span v-if="blockedReason" class="footer-hint">{{ blockedReason }}</span>
        <button class="btn" type="button" @click="optiScalerDialog = null">{{ t('cancel') }}</button>
        <button class="btn btn-primary" :class="{ 'is-loading': moduleBusy }" type="button" :disabled="!optiScalerDialog.preview.canApply || dialogBlocked" @click="applyOptiScaler">
          {{ moduleBusy ? t('previewWorking') : (optiScalerDialog.preview.installed ? t('actionReinstall') : t('previewInstall')) }}
        </button>
      </template>
    </BaseDialog>

    <BaseDialog
      v-if="cheekyDialog"
      :eyebrow="t('previewEyebrow')"
      :title="moduleNameById('cheeky-foveated-dlss')"
      :description="cheekyDialog.request.gameName"
      @close="cheekyDialog = null"
    >
      <div class="fact-grid">
        <div class="fact"><span>{{ t('factVersion') }}</span><strong>{{ cheekyDialog.request.version }}</strong></div>
        <div class="fact"><span>{{ t('factLoadedVia') }}</span><strong>{{ t('cheekyReshadeAddon') }}</strong></div>
      </div>
      <ChangePreview :changes="cheekyDialog.preview.changes" :warnings="cheekyDialog.preview.warnings" :location="cheekyDialog.preview.addonPath" />
      <template #footer>
        <span v-if="blockedReason" class="footer-hint">{{ blockedReason }}</span>
        <button class="btn" type="button" @click="cheekyDialog = null">{{ t('cancel') }}</button>
        <button class="btn btn-primary" :class="{ 'is-loading': moduleBusy }" type="button" :disabled="!cheekyDialog.preview.canApply || dialogBlocked" @click="applyCheeky">
          {{ moduleBusy ? t('previewWorking') : (cheekyDialog.preview.installed ? t('actionReinstall') : t('previewInstall')) }}
        </button>
      </template>
    </BaseDialog>

    <BaseDialog
      v-if="uevrDialog"
      :eyebrow="t('previewEyebrow')"
      :title="moduleNameById('uevr')"
      :description="uevrDialog.request.gameName"
      @close="uevrDialog = null"
    >
      <div class="fact-grid">
        <div class="fact">
          <span>{{ t('advancedEngine') }}</span>
          <strong>{{ uevrDialog.preview.engine ?? t('unknownEngine') }}{{ uevrDialog.preview.engineVersion ? ` · ${uevrDialog.preview.engineVersion}` : '' }}</strong>
        </div>
        <div class="fact"><span>{{ t('uevrBackend') }}</span><strong>{{ uevrBackendLabel(uevrDialog.request.backend) }}</strong></div>
        <div class="fact"><span>{{ t('factVersion') }}</span><strong>{{ uevrDialog.preview.selectedVersion ?? t('notDetected') }}</strong></div>
      </div>
      <ChangePreview :changes="uevrDialog.preview.changes" :warnings="uevrDialog.preview.warnings" :location="uevrDialog.preview.installDirectory" />
      <template #footer>
        <span v-if="blockedReason" class="footer-hint">{{ blockedReason }}</span>
        <button class="btn" type="button" @click="uevrDialog = null">{{ t('cancel') }}</button>
        <button class="btn btn-primary" :class="{ 'is-loading': moduleBusy }" type="button" :disabled="!uevrDialog.preview.canApply || dialogBlocked" @click="applyUevr">
          {{ moduleBusy ? t('previewWorking') : (uevrDialog.preview.installed ? t('actionReinstall') : t('previewInstall')) }}
        </button>
      </template>
    </BaseDialog>

    <BaseDialog
      v-if="ofxrDialog"
      :eyebrow="t('previewEyebrow')"
      :title="moduleNameById('ofxr-framegen')"
      :description="ofxrDialog.request.gameName"
      @close="ofxrDialog = null"
    >
      <div class="fact-grid">
        <div class="fact"><span>{{ t('factVersion') }}</span><strong>{{ ofxrDialog.request.version }}</strong></div>
        <div class="fact"><span>{{ t('ofxrMode') }}</span><strong>{{ ofxrDialog.request.backend }}</strong></div>
        <div class="fact"><span>{{ t('factStatus') }}</span><strong>{{ ofxrDialog.preview.armed ? t('ofxrConfiguredState') : t('ofxrNotConfiguredState') }}</strong></div>
      </div>
      <ChangePreview
        :changes="ofxrDialog.preview.changes"
        :warnings="ofxrDialog.preview.warnings"
        :location="ofxrDialog.preview.installDirectory"
        :empty-text="t('ofxrReadyForLaunch')"
      />
      <template #footer>
        <span v-if="blockedReason" class="footer-hint">{{ blockedReason }}</span>
        <button class="btn" type="button" @click="ofxrDialog = null">{{ t('cancel') }}</button>
        <button class="btn btn-primary" :class="{ 'is-loading': moduleBusy }" type="button" :disabled="!ofxrDialog.preview.canApply || dialogBlocked" @click="applyOfxr">
          {{ moduleBusy ? t('previewWorking') : t('ofxrInstallAndArm') }}
        </button>
      </template>
    </BaseDialog>

    <BaseDialog
      v-if="vrLaunchDialog"
      :eyebrow="t('vrLaunchEyebrow')"
      :title="vrLaunchDialog.request.gameName"
      :description="t('vrLaunchDescription')"
      @close="vrLaunchDialog = null"
    >
      <div class="fact-grid">
        <div class="fact"><span>{{ t('vrRuntime') }}</span><strong>{{ vrLaunchDialog.preview.activeOpenXrRuntime ?? t('notDetected') }}</strong></div>
        <div v-if="vrLaunchDialog.request.arguments.length" class="fact">
          <span>{{ t('launchArguments') }}</span><strong>{{ vrLaunchDialog.request.arguments.join(' ') }}</strong>
        </div>
      </div>

      <div v-if="ofxrLaunchRequest" class="callout callout-info">
        <AppIcon class="callout-icon" name="info" />
        <div>
          <strong>{{ t('ofxrBeforeLaunch') }}</strong>
          <p>{{ ofxrReadyForLaunch ? t('ofxrReadyForLaunch') : t('ofxrWillPrepare') }}</p>
        </div>
      </div>

      <section v-if="vrLaunchDialog.request.recommendations.length" class="dialog-section">
        <h3>{{ t('recommendedGraphics') }}</h3>
        <ul class="note-list">
          <li v-for="item in vrLaunchDialog.request.recommendations" :key="`${item.label}-${item.value}`">
            <strong>{{ item.label }}:</strong> {{ item.value }}
          </li>
        </ul>
      </section>

      <section v-if="vrLaunchDialog.preview.settings.some((setting) => setting.willChange)" class="dialog-section">
        <h3>{{ t('settingsToApply') }}</h3>
        <ul class="step-list setting-list">
          <li v-for="setting in vrLaunchDialog.preview.settings.filter((item) => item.willChange)" :key="`${setting.section}-${setting.key}`">
            <span>{{ setting.key }}</span>
            <span class="from">{{ setting.currentValue ?? t('notConfigured') }}</span>
            <span aria-hidden="true">→</span>
            <span class="to">{{ setting.value }}</span>
          </li>
        </ul>
      </section>

      <ChangePreview
        :changes="vrLaunchDialog.preview.changes"
        :warnings="vrLaunchDialog.preview.warnings"
        :location="vrLaunchDialog.preview.configPath"
        :show-backup-note="Boolean(vrLaunchDialog.preview.configPath)"
      />
      <template #footer>
        <span v-if="blockedReason" class="footer-hint">{{ blockedReason }}</span>
        <button class="btn" type="button" @click="vrLaunchDialog = null">{{ t('cancel') }}</button>
        <button class="btn btn-primary" :class="{ 'is-loading': moduleBusy }" type="button" :disabled="!vrLaunchDialog.preview.canLaunch || dialogBlocked" @click="launchVrGame">
          <AppIcon v-if="!moduleBusy" name="play" />
          {{ moduleBusy ? t('launching') : (ofxrReadyForLaunch ? t('launchVr') : t('activateAndLaunch')) }}
        </button>
      </template>
    </BaseDialog>

    <BaseDialog
      v-if="cheekyGuideDialog"
      size="lg"
      :eyebrow="moduleNameById('cheeky-foveated-dlss')"
      :title="t('compatibilityGuide')"
      :description="t('cheekyGuideResearchBased')"
      @close="cheekyGuideDialog = null"
    >
      <div class="callout" :class="cheekyResearchGuide(cheekyGuideDialog).state === 'prerequisite' ? 'callout-warning' : 'callout-info'">
        <AppIcon class="callout-icon" :name="cheekyResearchGuide(cheekyGuideDialog).state === 'prerequisite' ? 'alert' : 'info'" />
        <div>
          <strong>{{ cheekyResearchStateLabel(cheekyResearchGuide(cheekyGuideDialog).state) }}</strong>
          <p>{{ cheekyResearchGuide(cheekyGuideDialog).decision }}</p>
        </div>
      </div>
      <section class="dialog-section">
        <h3>{{ t('cheekyGuideRoute') }}</h3>
        <p>{{ cheekyResearchGuide(cheekyGuideDialog).route }}</p>
      </section>
      <section class="dialog-section">
        <h3>{{ t('cheekyGuidePrerequisites') }}</h3>
        <ul class="change-list">
          <li v-for="item in cheekyResearchGuide(cheekyGuideDialog).prerequisites" :key="item">
            <AppIcon name="check" :size="14" /><span>{{ item }}</span>
          </li>
        </ul>
      </section>
      <section class="dialog-section">
        <h3>{{ t('cheekyGuideTestChecklist') }}</h3>
        <ol class="step-list">
          <li v-for="item in cheekyResearchGuide(cheekyGuideDialog).validation" :key="item">{{ item }}</li>
        </ol>
      </section>
      <div class="callout callout-warning">
        <AppIcon class="callout-icon" name="alert" />
        <div>
          <strong>{{ t('cheekyGuideRisks') }}</strong>
          <p>{{ cheekyResearchGuide(cheekyGuideDialog).risk }}</p>
        </div>
      </div>
      <template #footer>
        <button class="btn" type="button" @click="cheekyGuideDialog = null">{{ t('close') }}</button>
        <button class="btn btn-primary" type="button" @click="openCompatibilityReport(cheekyGuideDialog)">{{ t('recordTest') }}</button>
      </template>
    </BaseDialog>

    <BaseDialog
      v-if="compatibilityDialog"
      :eyebrow="t('compatibilityTestTitle')"
      :title="compatibilityDialog.gameName"
      :description="t('compatibilityTestDescription', { module: moduleName(compatibilityDialog.module), version: compatibilityDialog.version })"
      @close="compatibilityDialog = null"
    >
      <label class="field">
        <span>{{ t('compatibilityTestStatus') }}</span>
        <select v-model="compatibilityDraftStatus" class="select">
          <option v-for="status in COMPATIBILITY_STATUSES" :key="status" :value="status">{{ compatibility.statusLabel(status) }}</option>
        </select>
      </label>
      <label class="field">
        <span>{{ t('compatibilityTestNotes') }}</span>
        <textarea v-model="compatibilityDraftNote" class="textarea" rows="4" :placeholder="t('compatibilityTestNotesPlaceholder')" />
        <small>{{ t('compatibilityTestPrivacy') }}</small>
      </label>
      <template #footer>
        <button class="btn" type="button" @click="compatibilityDialog = null">{{ t('cancel') }}</button>
        <button class="btn btn-primary" type="button" @click="compatibility.save()">{{ t('saveTestResult') }}</button>
      </template>
    </BaseDialog>

    <BaseDialog
      v-if="dependencyDialog"
      :eyebrow="t('moduleDependenciesEyebrow')"
      :title="moduleName(dependencyDialog.module)"
      :description="t('moduleDependenciesDescription')"
      @close="dependencyDialog = null"
    >
      <ul class="dependency-list">
        <li v-for="dependency in dependencyDialog.missing" :key="dependency.id">
          <span>
            <strong>{{ moduleName(dependency) }}</strong>
            <small>{{ moduleDescription(dependency) }}</small>
          </span>
          <button
            class="btn btn-sm"
            :class="{ 'is-loading': busyModuleId === dependency.id }"
            type="button"
            :disabled="moduleBusy"
            @click="installDependency(dependency)"
          >
            {{ t('actionInstall') }}
          </button>
        </li>
      </ul>
      <template #footer>
        <button class="btn" type="button" @click="dependencyDialog = null">{{ t('close') }}</button>
      </template>
    </BaseDialog>

    <DesktopShortcutDialog
      v-if="desktopShortcut.dialog.value"
      :module-name="moduleName(desktopShortcut.dialog.value.module)"
      :preview="desktopShortcut.dialog.value.preview"
      :busy="moduleBusy"
      :disabled="inspectionLoading || selectedGameRunning"
      @close="desktopShortcut.close"
      @confirm="desktopShortcut.confirm"
    />

    <AiAssistantDialog />
  </div>
</template>

<style scoped>
.library-layout {
  display: grid;
  flex: 1;
  grid-template-columns: minmax(240px, 300px) minmax(0, 1fr);
  gap: var(--moddin-space-4);
  min-height: 0;
}

.game-detail {
  display: flex;
  flex-direction: column;
  gap: var(--moddin-space-5);
  overflow-y: auto;
  padding: var(--moddin-space-6);
}
.game-detail-empty { flex: 1; }

.game-hero { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--moddin-space-5); }
.game-hero-copy { display: grid; gap: var(--moddin-space-2); min-width: 0; }
.game-hero h1 { overflow-wrap: anywhere; }
.game-hero-actions { display: flex; align-items: center; gap: var(--moddin-space-3); flex-wrap: wrap; }

.game-progress { display: flex; flex-wrap: wrap; align-items: center; gap: var(--moddin-space-3); color: var(--moddin-text-muted); font-size: var(--moddin-text-md); }
.progress-track { width: 120px; height: 6px; overflow: hidden; border-radius: var(--moddin-radius-pill); background: var(--moddin-surface-3); }
.progress-track span { display: block; height: 100%; border-radius: inherit; background: var(--moddin-success); transition: width var(--moddin-normal) var(--moddin-ease); }

.mods-section { display: grid; gap: var(--moddin-space-5); }
.section-heading { display: flex; flex-wrap: wrap; align-items: flex-end; justify-content: space-between; gap: var(--moddin-space-3); }
.section-heading p { margin-top: 2px; color: var(--moddin-text-muted); font-size: var(--moddin-text-md); }

.mod-group { display: grid; gap: var(--moddin-space-3); }
.mod-group-title { color: var(--moddin-text-muted); font-size: var(--moddin-text-xs); font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; }
.mod-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: var(--moddin-space-3); }

.detail-block { display: grid; gap: var(--moddin-space-2); }
.detail-title { margin: 0; color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }
.detail-copy { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }
.detail-row { display: flex; flex-wrap: wrap; align-items: center; gap: var(--moddin-space-2); }
.detail-row.spread { justify-content: space-between; }

.advanced-panel { border: 1px solid var(--moddin-line-soft); border-radius: var(--moddin-radius-lg); padding: var(--moddin-space-4); }
.advanced-panel > summary { color: var(--moddin-text-soft); font-size: var(--moddin-text-md); font-weight: 650; }
.advanced-panel > summary small { color: var(--moddin-text-faint); font-weight: 500; }
.advanced-body { display: grid; gap: var(--moddin-space-4); margin-top: var(--moddin-space-4); }

.info-list { display: grid; gap: var(--moddin-space-3); margin: 0; }
.info-list > div { display: grid; grid-template-columns: 180px minmax(0, 1fr); gap: var(--moddin-space-3); }
.info-list dt { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }
.info-list dd { display: flex; flex-wrap: wrap; gap: var(--moddin-space-2); margin: 0; color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }

.dll-chip { border-radius: var(--moddin-radius-sm); padding: 2px 8px; color: var(--moddin-warning); background: var(--moddin-warning-bg); font-family: var(--moddin-mono); font-size: var(--moddin-text-xs); }

.dependency-list { display: grid; gap: var(--moddin-space-2); margin: 0; padding: 0; list-style: none; }
.dependency-list li { display: flex; align-items: center; justify-content: space-between; gap: var(--moddin-space-3); padding: var(--moddin-space-2) var(--moddin-space-3); border: 1px solid var(--moddin-line); border-radius: var(--moddin-radius-md); }
.dependency-list span { display: grid; gap: 2px; }
.dependency-list small { color: var(--moddin-text-muted); font-size: var(--moddin-text-xs); }

.opti-replace { margin-top: var(--moddin-space-2); }
.opti-replace span { display: flex; align-items: center; gap: var(--moddin-space-2); }
.opti-replace input { width: 15px; height: 15px; accent-color: var(--moddin-accent); }

@media (max-width: 960px) {
  .library-layout { display: flex; flex: none; flex-direction: column; }
  .library-layout :deep(.game-list) { flex: none; height: 300px; }
  .game-detail { flex: none; overflow: visible; padding: var(--moddin-space-4); }
  .game-hero { flex-direction: column; }
  .info-list > div { grid-template-columns: 1fr; gap: 2px; }
}
</style>
