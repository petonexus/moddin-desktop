<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invokeDebug as invoke } from './debug'
import { findCatalogGameByInstalledGame, gameCatalog } from './services/catalog'
import { localeOptions } from './i18n'
import type { InstalledGame, ModuleCategory, ToolModuleDefinition } from './types/game'
import type { GameEnvironmentInspection } from './types/inspection'
import type { ObsVrPreview, ObsVrRequest } from './types/obs'
import type { OptiScalerPreview, OptiScalerRequest } from './types/optiscaler'
import type { OfxrPreview, OfxrRequest, OfxrResult } from './types/ofxr'
import type { CheekyFoveatedDlssPreview, CheekyFoveatedDlssRequest, CheekyFoveatedDlssResult } from './types/cheeky'
import type { UevrBackend, UevrBackendCompatibility, UevrPreview, UevrRequest, UevrResult } from './types/uevr'
import type { TransactionRecord } from './types/transaction'
import type { VrIniPatch, VrLaunchPreview, VrLaunchRequest, VrLaunchResult, VrRecommendation } from './types/vr-launch'
import DesktopShortcutDialog from './features/desktop-shortcut/DesktopShortcutDialog.vue'
import AiAssistantDialog from './components/shell/AiAssistantDialog.vue'
import AiAssistantTrigger from './components/shell/AiAssistantTrigger.vue'
import AiGameSuggestions from './components/shell/AiGameSuggestions.vue'
import AiTopbarMenu from './components/shell/AiTopbarMenu.vue'
import { useAiTopbarActions } from './composables/useAiTopbarActions'
import { useDesktopShortcut } from './features/desktop-shortcut/useDesktopShortcut'
import type { ModuleVerification, ModuleVerificationCheck } from './types/module-verification'
import type { ModuleUpdate } from './types/module-update'
import type { CompatibilityReport, CompatibilityStatus } from './types/compatibility'
import { version as appVersion } from '../package.json'
import AppIcon from './components/ui/AppIcon.vue'
import BaseDialog from './components/ui/BaseDialog.vue'
import ChangePreview from './components/ui/ChangePreview.vue'
import ToastStack from './components/ui/ToastStack.vue'
import GlobalTools from './components/shell/GlobalTools.vue'
import GameList from './features/library/GameList.vue'
import ModuleCard, { type ModuleCardState, type ModuleCardUpdate } from './features/library/ModuleCard.vue'
import HistoryView from './features/history/HistoryView.vue'

type ViewName = 'library' | 'history'
type CheekyResearchState = 'experimental' | 'prerequisite'
type ModuleFilter = 'all' | ModuleCategory

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
const verificationBusyKey = ref<string | null>(null)
const verificationBusyKeys = ref(new Set<string>())
const rollbackBusyId = ref<string | null>(null)
const moduleVerifications = ref<Record<string, ModuleVerification>>({})
const updateBusyKeys = ref(new Set<string>())
const moduleUpdates = ref<Record<string, ModuleUpdate>>({})
const compatibilityReports = ref<Record<string, CompatibilityReport>>({})
const SELECTION_DEBOUNCE_MS = 180
const BACKGROUND_TASK_DELAY_MS = 220
const GAME_STATE_POLL_MS = 3000
const AUTO_VERIFICATION_TTL_MS = 15_000
const AUTO_UPDATE_TTL_MS = 10 * 60_000
const UPDATE_CHECK_TIMEOUT_MS = 35_000
const BACKGROUND_CONCURRENCY = 2
let gameStatePoll: number | null = null
let selectionDebounce: number | null = null
let selectionBackgroundTimer: number | null = null
let selectionGeneration = 0
let appUnmounted = false
const inspectionRequests = new Map<string, Promise<GameEnvironmentInspection>>()
const obsDialog = ref<{ request: ObsVrRequest; preview: ObsVrPreview } | null>(null)
const optiScalerDialog = ref<{ request: OptiScalerRequest; preview: OptiScalerPreview } | null>(null)
const ofxrDialog = ref<{ request: OfxrRequest; preview: OfxrPreview } | null>(null)
const cheekyDialog = ref<{ request: CheekyFoveatedDlssRequest; preview: CheekyFoveatedDlssPreview } | null>(null)
const uevrDialog = ref<{ request: UevrRequest; preview: UevrPreview } | null>(null)
const cheekyGuideDialog = ref<ToolModuleDefinition | null>(null)
const compatibilityDialog = ref<{ module: ToolModuleDefinition; gameName: string; version: string } | null>(null)
const compatibilityDraftStatus = ref<CompatibilityStatus>('unverified')
const compatibilityDraftNote = ref('')
const vrLaunchDialog = ref<{ request: VrLaunchRequest; preview: VrLaunchPreview } | null>(null)
const desktopShortcut = useDesktopShortcut({
  busy: moduleBusy,
  actionError,
  success,
  onCreated: async (module) => {
    await refreshTransactions()
    await verifyModule(module, true)
  },
})

// AI topbar wiring: the menu emits dumb events; this composable turns
// them into AI dialog opens (audit / diagnose / contribute / etc.).
const aiTopbar = useAiTopbarActions({
  selectedAppId: () => selectedAppId.value,
  selectedGameName: () => selectedGame.value?.catalog?.name ?? null,
  currentError: () => (typeof error.value === 'string' ? error.value : null),
})
const uevrBackendSelections = ref<Record<string, UevrBackend>>({})

const compatibilityStatuses: CompatibilityStatus[] = [
  'unverified',
  'experimental',
  'proven',
  'risky',
  'not_working',
]

const moduleCategories: ModuleCategory[] = ['vr', 'graphics', 'qol', 'system']

const { t, locale } = useI18n()

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
  return modules.some((module) => moduleVerification(module)?.gameRunning === true)
})

const selectedOfxrModule = computed(() =>
  selectedGame.value?.catalog?.modules.find((module) => module.id === 'ofxr-framegen') ?? null,
)

const ofxrLaunchRequest = computed(() => {
  const module = selectedOfxrModule.value
  return module ? buildOfxrRequest(module) : null
})

const ofxrReadyForLaunch = computed(() => {
  const module = selectedOfxrModule.value
  return module ? moduleVerification(module)?.status === 'installed' : true
})

const primaryModule = computed(() => {
  const modules = selectedGame.value?.catalog?.modules ?? []
  return modules.find((module) => module.id === 'vr-launch' && module.status === 'available')
    ?? modules.find((module) => module.status === 'available')
    ?? modules.find((module) => module.id === 'vr-launch')
    ?? null
})

const moduleProgress = computed(() => {
  const states = (selectedGame.value?.catalog?.modules ?? [])
    .filter((module) => module.status === 'available')
    .map(moduleCardState)
  return {
    total: states.length,
    active: states.filter((state) => state === 'active').length,
    attention: states.filter((state) => state === 'attention').length,
    checking: states.includes('checking'),
  }
})

function moduleCardState(module: ToolModuleDefinition): ModuleCardState {
  if (module.status !== 'available') return 'planned'
  const verification = moduleVerification(module)
  if (!verification) return moduleVerificationPending(module) ? 'checking' : 'unknown'
  if (verification.status === 'installed') return 'active'
  if (verification.status === 'ready') return 'available'
  if (verification.status === 'attention') return 'attention'
  return 'unknown'
}

function moduleCardTag(module: ToolModuleDefinition): { label: string; tone: 'success' | 'warning' | 'danger' | 'neutral' } | null {
  // Show a compatibility chip for any module that carries an explicit
  // `compatibilityStatus` in its catalog config — not only Cheeky.
  // Catalog authors can mark research findings as `proven`, `experimental`,
  // `risky`, or `not_working` per-game; the chip surfaces that for the player.
  const hasExplicitCompat = Object.prototype.hasOwnProperty.call(module.config ?? {}, 'compatibilityStatus')
  if (!hasExplicitCompat) {
    // Legacy path: keep the original Cheeky-only chip behaviour so older
    // catalog entries (without an explicit compatibilityStatus) still
    // surface a 'unverified' chip when the engine preset says so.
    if (module.id !== 'cheeky-foveated-dlss') return null
  }
  const status = compatibilityStatus(module)
  const tone = status === 'proven' ? 'success' : status === 'experimental' ? 'warning' : status === 'unverified' ? 'neutral' : 'danger'
  return { label: compatibilityStatusLabel(status), tone }
}

function moduleCardUpdate(module: ToolModuleDefinition): ModuleCardUpdate | null {
  // Hide the whole update area when the recipe has nowhere to look; showing
  // "no update source" on every card was noise the user could not act on.
  if (module.status !== 'available' || !moduleHasUpdateSource(module)) return null
  const update = moduleUpdate(module)
  const available = update?.status === 'available'
  return {
    hasSource: true,
    available,
    summary: available ? t('updateAvailableShort', { version: update?.latestVersion ?? '?' }) : moduleUpdateSummary(module),
    releaseUrl: update?.releaseUrl ?? null,
    busy: moduleUpdateBusy(module),
  }
}

function moduleRemoveLabel(module: ToolModuleDefinition) {
  return module.status === 'available' && activeModuleTransaction(module) ? t('actionRemove') : null
}

const availableModuleCategories = computed(() => {
  const modules = selectedGame.value?.catalog?.modules ?? []
  return moduleCategories.filter((category) => modules.some((module) => module.category === category))
})

const moduleGroups = computed(() => {
  const modules = selectedGame.value?.catalog?.modules ?? []
  return availableModuleCategories.value
    .filter((category) => activeModuleFilter.value === 'all' || activeModuleFilter.value === category)
    .map((category) => ({
      category,
      modules: modules.filter((module) => module.category === category),
    }))
    .filter((group) => group.modules.length > 0)
})

function moduleCategoryCount(category: ModuleCategory) {
  return (selectedGame.value?.catalog?.modules ?? []).filter((module) => module.category === category).length
}

const localizedModuleNames: Record<string, string> = {
  'vr-launch': 'moduleVrLaunch',
  'obs-vr': 'moduleObsVr',
  optiscaler: 'moduleOptiScaler',
  'ofxr-framegen': 'moduleOfxr',
  'cheeky-foveated-dlss': 'moduleCheeky',
  uevr: 'moduleUevr',
  openxr: 'moduleOpenXr',
  'desktop-shortcut': 'moduleDesktopShortcut',
}

function moduleNameById(id: string, fallback = id) {
  const key = localizedModuleNames[id]
  return key ? t(key) : fallback
}

function moduleName(module: ToolModuleDefinition) {
  return moduleNameById(module.id, module.name)
}

function gameNameById(gameId: string) {
  return gameCatalog.find((game) => game.id === gameId)?.name ?? gameId
}

function transactionBlockedReason(transaction: TransactionRecord) {
  return selectedGameRunning.value && transaction.gameId === selectedGame.value?.catalog?.id
    ? t('gameRunningBlocked')
    : undefined
}

function moduleDescription(module: ToolModuleDefinition) {
  if (module.id === 'vr-launch') return t('moduleVrLaunchDescription')
  if (module.id === 'obs-vr') return t('moduleObsVrDescription')
  if (module.id === 'optiscaler') return t('moduleOptiScalerDescription')
  if (module.id === 'ofxr-framegen') return t('moduleOfxrDescription')
  if (module.id === 'cheeky-foveated-dlss') return t('moduleCheekyDescription')
  if (module.id === 'uevr') return t('moduleUevrDescription')
  if (module.id === 'openxr') return t('moduleOpenXrDescription')
  return module.description
}

function categoryLabel(category: ToolModuleDefinition['category']) {
  return t(`category${category.charAt(0).toUpperCase()}${category.slice(1)}`)
}

function moduleKey(module: ToolModuleDefinition) {
  return moduleKeyForApp(module, selectedAppId.value)
}

function moduleKeyForApp(module: ToolModuleDefinition, appId: string | null) {
  const game = gameContextForAppId(appId)
  return `${game?.catalog.id ?? appId ?? 'unknown'}:${module.id}`
}

function moduleVerification(module: ToolModuleDefinition) {
  return moduleVerifications.value[moduleKey(module)]
}

function moduleVerificationBusy(module: ToolModuleDefinition) {
  const key = moduleKey(module)
  return verificationBusyKey.value === key || verificationBusyKeys.value.has(key)
}

function moduleVerificationPending(module: ToolModuleDefinition) {
  return module.status === 'available'
    && !moduleVerification(module)
    && (inspectionLoading.value || moduleVerificationBusy(module))
}

function isCompatibilityStatus(value: unknown): value is CompatibilityStatus {
  return typeof value === 'string' && compatibilityStatuses.includes(value as CompatibilityStatus)
}

function defaultCompatibilityStatus(module: ToolModuleDefinition): CompatibilityStatus {
  const configured = module.config?.compatibilityStatus
  return isCompatibilityStatus(configured) ? configured : 'unverified'
}

function moduleVersion(module: ToolModuleDefinition) {
  return typeof module.config?.version === 'string' ? module.config.version : '?'
}

function compatibilityReport(module: ToolModuleDefinition) {
  const report = compatibilityReports.value[moduleKey(module)]
  return report?.testedVersion === moduleVersion(module) ? report : undefined
}

function compatibilityStatus(module: ToolModuleDefinition): CompatibilityStatus {
  return compatibilityReport(module)?.status ?? defaultCompatibilityStatus(module)
}

function compatibilityStatusLabel(status: CompatibilityStatus) {
  if (status === 'experimental') return t('compatibilityExperimental')
  if (status === 'proven') return t('compatibilityProven')
  if (status === 'risky') return t('compatibilityRisky')
  if (status === 'not_working') return t('compatibilityNotWorking')
  return t('compatibilityUnverified')
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
  const report = compatibilityReport(module)
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

  return `${t('compatibilityUpstreamEvidence')} ${cheekyGameCompatibilityNote(module)}`
}

function cheekyCompatibilityEvidence() {
  return `${t('compatibilityKnownWorking')} ${t('compatibilityKnownIssues')}`
}

function cheekyGameCompatibilityNote(module: ToolModuleDefinition) {
  if (module.id !== 'cheeky-foveated-dlss') return ''
  const gameId = selectedGame.value?.catalog?.id
  if (gameId === 'cyberpunk-2077') return t('cheekyNoteCyberpunk')
  if (gameId === 'elden-ring') return t('cheekyNoteEldenRing')
  if (gameId === 'stalker-2') return t('cheekyNoteStalker2')
  return ''
}

function openCompatibilityReport(module: ToolModuleDefinition) {
  cheekyGuideDialog.value = null
  const version = moduleVersion(module)
  const report = compatibilityReport(module)
  compatibilityDraftStatus.value = report?.status ?? defaultCompatibilityStatus(module)
  compatibilityDraftNote.value = report?.note ?? ''
  compatibilityDialog.value = {
    module,
    gameName: selectedGame.value?.catalog?.name ?? selectedGame.value?.installed.name ?? '',
    version,
  }
}

function saveCompatibilityReport() {
  const dialog = compatibilityDialog.value
  if (!dialog) return

  const key = moduleKey(dialog.module)
  const nextReports = { ...compatibilityReports.value }
  if (compatibilityDraftStatus.value === 'unverified') {
    delete nextReports[key]
  } else {
    nextReports[key] = {
      status: compatibilityDraftStatus.value,
      note: compatibilityDraftNote.value.trim(),
      testedVersion: dialog.version,
      updatedAt: Date.now(),
    }
  }
  compatibilityReports.value = nextReports
  compatibilityDialog.value = null
  success.value = t('compatibilityTestSaved')
}

function loadCompatibilityReports() {
  try {
    const raw = window.localStorage.getItem('moddin-compatibility-reports')
    if (!raw) return
    const parsed = JSON.parse(raw) as Record<string, unknown>
    const validReports: Record<string, CompatibilityReport> = {}
    for (const [key, value] of Object.entries(parsed)) {
      if (!value || typeof value !== 'object') continue
      const report = value as Partial<CompatibilityReport>
      if (!isCompatibilityStatus(report.status) || typeof report.note !== 'string' || typeof report.testedVersion !== 'string' || typeof report.updatedAt !== 'number') continue
      validReports[key] = {
        status: report.status,
        note: report.note,
        testedVersion: report.testedVersion,
        updatedAt: report.updatedAt,
      }
    }
    compatibilityReports.value = validReports
  } catch {
    // Ignore unavailable or malformed local storage.
  }
}

function moduleUpdate(module: ToolModuleDefinition) {
  return moduleUpdates.value[moduleKey(module)]
}

function moduleUpdateSource(module: ToolModuleDefinition): string | null {
  const config = module.config ?? {}
  const configured = config.updateUrl
  if (typeof configured === 'string' && configured.trim()) return configured.trim()

  if (module.id === 'uevr') {
    const backend = selectedUevrBackend(module)
    const sourceKey = backend === 'afw' || backend === 'joey-afw' ? 'afwReleaseApiUrl' : 'releaseApiUrl'
    const source = config[sourceKey]
    return typeof source === 'string' && source.trim() ? source.trim() : null
  }

  return null
}

function moduleHasUpdateSource(module: ToolModuleDefinition) {
  return Boolean(moduleUpdateSource(module))
}

async function openRelease(url: string | null) {
  if (!url) return

  try {
    await invoke('open_external_url', { url })
  } catch (err) {
    // Keep the link useful when the UI is running directly in a browser.
    const opened = window.open(url, '_blank', 'noopener,noreferrer')
    if (!opened) actionError.value = err instanceof Error ? err.message : String(err)
  }
}

function moduleUpdateBusy(module: ToolModuleDefinition) {
  return updateBusyKeys.value.has(moduleKey(module))
}

function moduleUpdateSummary(module: ToolModuleDefinition) {
  const update = moduleUpdate(module)
  if (!moduleHasUpdateSource(module)) return t('updateNoSource')
  if (!update) return t('updateNotChecked')
  if (update.status === 'available') {
    return t('updateAvailableSummary', { version: update.latestVersion ?? '?' })
  }
  if (update.status === 'current') {
    return t('updateCurrentSummary', { version: update.latestVersion ?? update.currentVersion ?? '?' })
  }
  if (update.status === 'unavailable') return t('updateNoSource')
  if (update.status === 'error') return t('updateFailed')
  if (update.status === 'unknown' && update.latestVersion) {
    return t('updateUnknownLocalSummary', { version: update.latestVersion })
  }
  return t('updateUnknownLocal')
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error(message)), timeoutMs)
    promise.then(resolve, reject).finally(() => window.clearTimeout(timer))
  })
}

function moduleActionsBlocked(module: ToolModuleDefinition) {
  return module.status !== 'available'
    || moduleBusy.value
    || inspectionLoading.value
    || selectedGameRunning.value
}

function moduleActionLabel(module: ToolModuleDefinition) {
  if (module.status !== 'available') return t('statePlanned')
  if (busyModuleId.value === module.id) return t('actionOpening')
  if (module.id === 'vr-launch') return t('actionLaunchVr')
  if (module.id === 'desktop-shortcut') return t('actionCreateShortcut')
  if (moduleVerification(module)?.status === 'installed') {
    return module.id === 'optiscaler' || module.id === 'uevr' ? t('actionReinstall') : t('actionApplyAgain')
  }
  return t('actionInstall')
}

function moduleActionPrimary(module: ToolModuleDefinition) {
  // Once a mod is active, re-running it is a secondary gesture.
  return module.id === 'vr-launch' || moduleVerification(module)?.status !== 'installed'
}

function moduleTransactionKind(module: ToolModuleDefinition) {
  if (module.id === 'obs-vr') return 'obs-vr'
  if (module.id === 'optiscaler') return 'optiscaler'
  if (module.id === 'ofxr-framegen') return 'ofxr-framegen'
  if (module.id === 'cheeky-foveated-dlss') return 'cheeky-foveated-dlss'
  if (module.id === 'uevr') return 'uevr'
  if (module.id === 'vr-launch') return 'vr-launch'
  if (module.id === 'desktop-shortcut') return 'desktop-shortcut'
  return null
}

function activeModuleTransaction(module: ToolModuleDefinition) {
  const gameId = selectedGame.value?.catalog?.id
  const kind = moduleTransactionKind(module)
  if (!gameId || !kind) return null
  return transactions.value.find((transaction) =>
    transaction.status === 'applied' && transaction.gameId === gameId && transaction.kind === kind,
  ) ?? null
}

function check(label: string, passed: boolean, detail?: string): ModuleVerificationCheck {
  return { label, passed, detail }
}

function saveModuleVerification(
  module: ToolModuleDefinition,
  status: ModuleVerification['status'],
  summary: string,
  checks: ModuleVerificationCheck[],
  gameRunning: boolean,
  expectedAppId = selectedAppId.value,
  expectedGeneration = selectionGeneration,
) {
  if (selectedAppId.value !== expectedAppId || selectionGeneration !== expectedGeneration) return
  moduleVerifications.value[moduleKeyForApp(module, expectedAppId)] = {
    status,
    summary,
    checks,
    gameRunning,
    checkedAt: Date.now(),
    activeTransactionId: activeModuleTransaction(module)?.id ?? null,
  }
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
    error.value = err instanceof Error ? err.message : String(err)
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
      void verifyAvailableModules(expectedAppId, expectedGeneration, true)
    }
  } catch (err) {
    if (!silent && selectedAppId.value === expectedAppId && selectionGeneration === expectedGeneration) {
      inspectionError.value = err instanceof Error ? err.message : String(err)
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
    actionError.value = err instanceof Error ? err.message : String(err)
  } finally {
    transactionsLoading.value = false
  }
}

function buildObsRequest(module: ToolModuleDefinition): ObsVrRequest | null {
  if (module.id !== 'obs-vr' || !selectedGame.value?.catalog) return null

  const config = module.config ?? {}
  const executableName = typeof config.executableName === 'string'
    ? config.executableName
    : selectedGame.value.catalog.executable.split(/[\\/]/).pop()
  if (!executableName) return null

  return {
    gameId: selectedGame.value.catalog.id,
    gameName: selectedGame.value.catalog.name,
    collectionName: typeof config.collectionName === 'string' ? config.collectionName : t('unnamedCollection'),
    sceneName: typeof config.sceneName === 'string' ? config.sceneName : 'vr',
    sourceName: typeof config.sourceName === 'string' ? config.sourceName : `${selectedGame.value.catalog.name} VR`,
    executableName,
  }
}

function optiScalerSafetyNotes(gameId: string): string[] {
  const notes = [t('optiSafetyOnline')]
  if (gameId === 'elden-ring') notes.push(t('optiSafetyEldenRing'))
  if (gameId === 'stalker-2') notes.push(t('optiSafetyStalker2'))
  return notes
}

function buildOptiScalerRequest(module: ToolModuleDefinition): OptiScalerRequest | null {
  const game = selectedGame.value
  if (module.id !== 'optiscaler' || !game?.catalog) return null

  const config = module.config ?? {}
  const version = typeof config.version === 'string' ? config.version : null
  const downloadUrl = typeof config.downloadUrl === 'string' ? config.downloadUrl : null
  const sha256 = typeof config.sha256 === 'string' ? config.sha256 : null
  if (!version || !downloadUrl || !sha256) return null

  const proxyCandidates = configList(config, 'proxyCandidates')

  if (!proxyCandidates.length) return null

  return {
    gameId: game.catalog.id,
    gameName: game.catalog.name,
    installDir: game.installed.installDir,
    executable: game.catalog.executable,
    version,
    downloadUrl,
    sha256,
    proxyCandidates,
    safetyNotes: optiScalerSafetyNotes(game.catalog.id),
  }
}

function cheekySafetyNotes(): string[] {
  return [t('cheekySafetyReshade'), t('cheekySafetyOpenXr'), t('cheekySafetyOneIntegration'), t('cheekySafetyEnableDlss')]
}

function buildCheekyRequest(module: ToolModuleDefinition): CheekyFoveatedDlssRequest | null {
  const game = selectedGame.value
  if (module.id !== 'cheeky-foveated-dlss' || !game?.catalog) return null

  const config = module.config ?? {}
  const version = typeof config.version === 'string' ? config.version : null
  const downloadUrl = typeof config.downloadUrl === 'string' ? config.downloadUrl : null
  const sha256 = typeof config.sha256 === 'string' ? config.sha256 : null
  const addonFile = typeof config.addonFile === 'string'
    ? config.addonFile
    : 'CheekyFoveatedDLSS.addon64'
  if (!version || !downloadUrl || !sha256 || !addonFile) return null

  return {
    gameId: game.catalog.id,
    gameName: game.catalog.name,
    installDir: game.installed.installDir,
    executable: game.catalog.executable,
    version,
    downloadUrl,
    sha256,
    addonFile,
    safetyNotes: [...cheekySafetyNotes(), cheekyGameCompatibilityNote(module)].filter(Boolean),
  }
}

async function configureModule(module: ToolModuleDefinition) {
  actionError.value = null
  success.value = null

  if (selectedGameRunning.value) {
    actionError.value = t('gameRunningActionBlocked')
    return
  }

  busyModuleId.value = module.id
  try {
    await openModulePreview(module)
  } finally {
    busyModuleId.value = null
  }
}

async function openModulePreview(module: ToolModuleDefinition) {
  if (await desktopShortcut.open(module, selectedGame.value)) return

  const vrRequest = buildVrLaunchRequest(module)
  if (vrRequest) {
    moduleBusy.value = true
    try {
      const preview = await invoke<VrLaunchPreview>('preview_vr_launch', { request: vrRequest })
      vrLaunchDialog.value = { request: vrRequest, preview }
    } catch (err) {
      actionError.value = err instanceof Error ? err.message : String(err)
    } finally {
      moduleBusy.value = false
    }
    return
  }

  moduleBusy.value = true
  try {
    if (module.id === 'obs-vr') {
      const request = buildObsRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      const preview = await invoke<ObsVrPreview>('preview_obs_vr', { request })
      obsDialog.value = { request, preview }
      return
    }

    if (module.id === 'optiscaler') {
      const request = buildOptiScalerRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      const preview = await invoke<OptiScalerPreview>('preview_optiscaler', { request })
      optiScalerDialog.value = { request, preview }
      return
    }

    if (module.id === 'ofxr-framegen') {
      const request = buildOfxrRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      const preview = await invoke<OfxrPreview>('preview_ofxr', { request })
      ofxrDialog.value = { request, preview }
      return
    }

    if (module.id === 'cheeky-foveated-dlss') {
      const request = buildCheekyRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      const preview = await invoke<CheekyFoveatedDlssPreview>('preview_cheeky_foveated_dlss', { request })
      cheekyDialog.value = { request, preview }
      return
    }

    if (module.id === 'uevr') {
      const request = buildUevrRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      const preview = await invoke<UevrPreview>('preview_uevr', { request })
      uevrDialog.value = { request, preview }
      return
    }

    throw new Error(t('moduleNoAction', { module: module.id }))
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
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
    const ofxrRequest = ofxrLaunchRequest.value
    const result = await invoke<VrLaunchResult>('launch_vr_game', {
      request,
      ofxrRequest,
    })
    vrLaunchDialog.value = null
    success.value = t('vrLaunchSuccess', { game: request.gameName })
    if (result.transaction) success.value += ` ${t('vrConfigSaved')}`
    await refreshTransactions()
    await verifyAvailableModules()
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
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
    const transaction = await invoke<TransactionRecord>('configure_obs_vr', {
      request: obsDialog.value.request,
    })
    obsDialog.value = null
    success.value = t('installSuccess', { name: configuredSource })
    await refreshTransactions()
    await verifyAvailableModules()
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
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
    const request = optiScalerDialog.value.request
    const transaction = await invoke<TransactionRecord>('install_optiscaler', { request })
    optiScalerDialog.value = null
    success.value = t('installSuccess', { name: `OptiScaler ${request.version}` })
    await refreshTransactions()
    await inspectSelectedGame()
    await verifyAvailableModules()
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
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
    const result = await invoke<OfxrResult>('install_ofxr', { request })
    if (!result.armed) throw new Error(t('ofxrActivationFailed'))
    ofxrDialog.value = null
    success.value = result.transaction
      ? t('installSuccess', { name: moduleNameById('ofxr-framegen') })
      : t('ofxrReadyForLaunch')
    await refreshTransactions()
    await verifyAvailableModules()
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
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
    const request = cheekyDialog.value.request
    const transaction = await invoke<CheekyFoveatedDlssResult>('install_cheeky_foveated_dlss', { request })
    cheekyDialog.value = null
    success.value = t('installSuccess', { name: moduleNameById('cheeky-foveated-dlss') })
    await refreshTransactions()
    await verifyAvailableModules()
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
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
    const transaction = await invoke<UevrResult>('install_uevr', { request })
    uevrDialog.value = null
    success.value = t('installSuccess', { name: `${uevrBackendLabel(request.backend)} ${transaction.metadata?.version ?? ''}`.trim() })
    await refreshTransactions()
    await verifyAvailableModules()
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
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
    await verifyAvailableModules()
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
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

function configList(config: ToolModuleDefinition['config'], key: string): string[] {
  const value = config?.[key]
  if (Array.isArray(value)) return value
  if (typeof value !== 'string' || !value.trim()) return []
  return value.split(',').map((item) => item.trim()).filter(Boolean)
}

function configBool(config: ToolModuleDefinition['config'], key: string, fallback = false): boolean {
  const value = config?.[key]
  if (typeof value !== 'string') return fallback
  return ['1', 'true', 'yes', 'on'].includes(value.trim().toLowerCase())
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
  const key = moduleKey(module)
  const selected = uevrBackendSelections.value[key]
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
  uevrBackendSelections.value = { ...uevrBackendSelections.value, [moduleKey(module)]: value }
  void verifyModule(module, true)
}

function onUevrBackendChange(module: ToolModuleDefinition, event: Event) {
  const target = event.target
  if (target instanceof HTMLSelectElement) setUevrBackend(module, target.value)
}

function buildUevrRequest(module: ToolModuleDefinition): UevrRequest | null {
  const game = selectedGame.value
  if (module.id !== 'uevr' || !game?.catalog) return null

  const config = module.config ?? {}
  const backend = selectedUevrBackend(module)
  const versionPolicy = config.versionPolicy === 'pinned' ? 'pinned' : 'latest'
  const releaseApiUrl = backend === 'afw' || backend === 'joey-afw'
    ? (typeof config.afwReleaseApiUrl === 'string'
      ? config.afwReleaseApiUrl
      : 'https://api.github.com/repos/PureDark/UEVR/releases/latest')
    : (typeof config.releaseApiUrl === 'string'
      ? config.releaseApiUrl
      : 'https://api.github.com/repos/praydog/UEVR-nightly/releases/latest')
  const backendReleaseApiUrl = typeof config.backendReleaseApiUrl === 'string'
    ? config.backendReleaseApiUrl
    : null
  const releaseTag = typeof config.releaseTag === 'string' && config.releaseTag.trim()
    ? config.releaseTag
    : null
  const backendReleaseTag = typeof config.backendReleaseTag === 'string' && config.backendReleaseTag.trim()
    ? config.backendReleaseTag
    : null

  return {
    gameId: game.catalog.id,
    gameName: game.catalog.name,
    installDir: game.installed.installDir,
    executable: game.catalog.executable,
    backend,
    versionPolicy,
    releaseApiUrl,
    releaseTag,
    backendReleaseApiUrl,
    backendReleaseTag,
    safetyNotes: configList(config, 'safetyNotes'),
  }
}

function buildOfxrRequest(module: ToolModuleDefinition): OfxrRequest | null {
  const game = selectedGame.value
  if (module.id !== 'ofxr-framegen' || !game?.catalog) return null

  const config = module.config ?? {}
  const version = typeof config.version === 'string' ? config.version : null
  const implementationVersion = typeof config.implementationVersion === 'string'
    ? Number(config.implementationVersion)
    : null
  const downloadUrl = typeof config.downloadUrl === 'string' ? config.downloadUrl : null
  const sha256 = typeof config.sha256 === 'string' ? config.sha256 : null
  const backend = typeof config.backend === 'string' ? config.backend : 'fidelityfx'
  const nvidiaPreset = typeof config.nvidiaPreset === 'string' ? config.nvidiaPreset : 'medium'
  const nvidiaInputScale = typeof config.nvidiaInputScale === 'string' ? Number(config.nvidiaInputScale) : 50
  if (!version || !implementationVersion || !downloadUrl || !sha256 || !Number.isFinite(nvidiaInputScale)) return null

  return {
    gameId: game.catalog.id,
    gameName: game.catalog.name,
    installDir: game.installed.installDir,
    executable: game.catalog.executable,
    version,
    implementationVersion,
    downloadUrl,
    sha256,
    backend,
    nvidiaPreset,
    nvidiaInputScale,
    nvidiaBidirectional: configBool(config, 'nvidiaBidirectional'),
    safetyNotes: configList(config, 'safetyNotes'),
  }
}

function buildVrLaunchRequest(module: ToolModuleDefinition): VrLaunchRequest | null {
  if (module.id !== 'vr-launch' || !selectedGame.value?.catalog) return null

  const config = module.config ?? {}
  const configPath = typeof config.configPath === 'string' ? config.configPath : null
  const configPatches: VrIniPatch[] = configList(config, 'configPatches').flatMap((value) => {
    const [section, key, ...parts] = value.split('|')
    if (!section || !key || !parts.length) return []
    return [{ section, key, value: parts.join('|') }]
  })
  const recommendations: VrRecommendation[] = configList(config, 'recommendations').map((value) => {
    const separator = value.indexOf('=')
    if (separator < 0) return { label: value, value: '' }
    return { label: value.slice(0, separator), value: value.slice(separator + 1) }
  })

  return {
    gameId: selectedGame.value.catalog.id,
    gameName: selectedGame.value.catalog.name,
    installDir: selectedGame.value.installed.installDir,
    executable: selectedGame.value.catalog.executable,
    arguments: configList(config, 'arguments'),
    requiredFiles: configList(config, 'requiredFiles'),
    configPath,
    configPatches,
    recommendations,
    safetyNotes: configList(config, 'safetyNotes'),
  }
}

function verificationSummary(status: ModuleVerification['status']) {
  if (status === 'installed') return t('verificationSummaryInstalled')
  if (status === 'ready') return t('verificationSummaryReady')
  if (status === 'attention') return t('verificationSummaryAttention')
  return t('verificationSummaryUnknown')
}

async function verifyModule(
  module: ToolModuleDefinition,
  silent = false,
  expectedAppId = selectedAppId.value,
  expectedGeneration = selectionGeneration,
) {
  if (module.status !== 'available') return
  if (selectedAppId.value !== expectedAppId || selectionGeneration !== expectedGeneration) return

  const key = moduleKeyForApp(module, expectedAppId)
  const busyKeys = new Set(verificationBusyKeys.value)
  busyKeys.add(key)
  verificationBusyKeys.value = busyKeys
  if (!silent) verificationBusyKey.value = key
  if (!silent) actionError.value = null

  try {
    if (module.id === 'obs-vr') {
      const request = buildObsRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      const preview = await invoke<ObsVrPreview>('preview_obs_vr', { request })
      const installed = preview.installed
      saveModuleVerification(module, installed ? 'installed' : preview.canApply ? 'ready' : 'attention', verificationSummary(installed ? 'installed' : preview.canApply ? 'ready' : 'attention'), [
        check(t('checkObsCollection'), Boolean(preview.collectionFile)),
        check(t('checkObsScene'), preview.sceneFound),
        check(t('checkObsSource'), preview.sourceExists),
        check(t('checkObsTarget'), preview.sourceTargetMatches),
        check(t('checkObsSceneLink'), preview.sourceInScene),
        check(t('checkGameClosed'), !preview.gameRunning),
      ], preview.gameRunning, expectedAppId, expectedGeneration)
    } else if (module.id === 'optiscaler') {
      const request = buildOptiScalerRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      const preview = await invoke<OptiScalerPreview>('preview_optiscaler', { request })
      const installed = preview.installed && preview.installedVersion === request.version
      const status: ModuleVerification['status'] = installed
        ? 'installed'
        : preview.canApply
          ? 'ready'
          : 'attention'
      saveModuleVerification(module, status, verificationSummary(status), [
        check(t('checkExecutable'), preview.executableExists),
        check(t('checkGameClosed'), !preview.gameRunning),
        check(t('checkManagedInstall'), !preview.manualInstallDetected),
        check(t('checkProxyAvailable'), Boolean(preview.selectedProxy)),
        check(t('checkVersion'), installed, preview.installed ? t('installedVersion', { version: preview.installedVersion }) : undefined),
      ], preview.gameRunning, expectedAppId, expectedGeneration)
    } else if (module.id === 'vr-launch') {
      const request = buildVrLaunchRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      const preview = await invoke<VrLaunchPreview>('preview_vr_launch', { request })
      const filesPresent = preview.missingFiles.length === 0
      const configMatches = !request.configPath
        || (preview.configExists && preview.settings.every((setting) => !setting.willChange))
      const status: ModuleVerification['status'] = filesPresent && configMatches
        ? 'installed'
        : preview.canLaunch
          ? 'ready'
          : 'attention'
      saveModuleVerification(module, status, verificationSummary(status), [
        check(t('checkExecutable'), preview.executableExists),
        check(t('checkVrFiles'), filesPresent),
        check(t('checkVrConfig'), configMatches),
        check(t('checkOpenXrRuntime'), Boolean(preview.activeOpenXrRuntime)),
        check(t('checkGameClosed'), !preview.gameRunning),
      ], preview.gameRunning, expectedAppId, expectedGeneration)
    } else if (module.id === 'desktop-shortcut') {
      const preview = await desktopShortcut.preview(module, selectedGame.value)
      const status: ModuleVerification['status'] = preview.canApply ? 'ready' : 'attention'
      saveModuleVerification(module, status, verificationSummary(status), [
        check(t('checkExecutable'), preview.canApply),
      ], false, expectedAppId, expectedGeneration)
    } else if (module.id === 'ofxr-framegen') {
      const request = buildOfxrRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      const preview = await invoke<OfxrPreview>('preview_ofxr', { request })
      const installed = preview.installed && preview.configured && preview.trayRunning && preview.armed
      const status: ModuleVerification['status'] = installed
        ? 'installed'
        : preview.canApply && preview.executableExists
          ? 'ready'
          : 'attention'
      saveModuleVerification(module, status, verificationSummary(status), [
        check(t('checkExecutable'), preview.executableExists),
        check(t('checkOfxrInstalled'), preview.installed && preview.trayInstalled),
        check(t('checkVersion'), preview.installed && preview.installedVersion === request.version, preview.installedVersion ? t('installedVersion', { version: preview.installedVersion }) : undefined),
        check(t('checkOfxrConfig'), preview.configured),
        check(t('checkOfxrTray'), preview.trayRunning),
        check(t('checkOfxrArmed'), preview.armed),
        check(t('checkGameClosed'), !preview.gameRunning),
        ], preview.gameRunning, expectedAppId, expectedGeneration)
    } else if (module.id === 'cheeky-foveated-dlss') {
      const request = buildCheekyRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      const preview = await invoke<CheekyFoveatedDlssPreview>('preview_cheeky_foveated_dlss', { request })
      const installed = preview.installed && preview.installedVersion === request.version
      const status: ModuleVerification['status'] = installed
        ? 'installed'
        : preview.canApply
          ? 'ready'
          : 'attention'
      saveModuleVerification(module, status, verificationSummary(status), [
        check(t('checkExecutable'), preview.executableExists),
        check(t('checkGameClosed'), !preview.gameRunning),
        check(t('checkManagedInstall'), !preview.manualInstallDetected),
        check(t('checkCheekyAddon'), preview.installed),
        check(t('checkVersion'), installed, preview.installed ? t('installedVersion', { version: preview.installedVersion }) : undefined),
      ], preview.gameRunning, expectedAppId, expectedGeneration)
    } else if (module.id === 'uevr') {
      const request = buildUevrRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      const preview = await invoke<UevrPreview>('preview_uevr', { request })
      const installed = preview.installed
        && preview.installedBackend === request.backend
        && preview.selectedVersion !== null
        && preview.installedVersion === preview.selectedVersion
      const status: ModuleVerification['status'] = installed
        ? 'installed'
        : preview.canApply
          ? 'ready'
          : 'attention'
      saveModuleVerification(module, status, verificationSummary(status), [
        check(t('checkExecutable'), preview.executableExists),
        check(t('checkUevrEngine'), preview.engine === 'Unreal Engine', preview.engine ?? t('uevrUnknownEngine')),
        check(t('checkUevrBackend'), preview.selectedVersion !== null, preview.backendLabel),
        check(t('checkGameClosed'), !preview.gameRunning && !preview.uevrRunning),
        check(t('checkManagedInstall'), !preview.manualInstallDetected),
        check(t('checkVersion'), installed, preview.installedVersion ? t('installedVersion', { version: preview.installedVersion }) : undefined),
      ], preview.gameRunning || preview.uevrRunning, expectedAppId, expectedGeneration)
    }

    if (!silent && selectedAppId.value === expectedAppId && selectionGeneration === expectedGeneration) {
      success.value = t('verificationCompleted', { module: moduleName(module) })
    }
  } catch (err) {
    if (!silent && selectedAppId.value === expectedAppId && selectionGeneration === expectedGeneration) {
      actionError.value = err instanceof Error ? err.message : String(err)
    }
  } finally {
    const nextBusyKeys = new Set(verificationBusyKeys.value)
    nextBusyKeys.delete(key)
    verificationBusyKeys.value = nextBusyKeys
    if (!silent && verificationBusyKey.value === key) verificationBusyKey.value = null
  }
}

async function verifyAvailableModules(
  expectedAppId = selectedAppId.value,
  expectedGeneration = selectionGeneration,
  force = false,
) {
  const game = gameContextForAppId(expectedAppId)
  if (!game || selectedAppId.value !== expectedAppId || selectionGeneration !== expectedGeneration) return
  const now = Date.now()
  const modules = game.catalog.modules.filter((module) => {
    if (module.status !== 'available' || module.id === 'uevr') return false
    const previous = moduleVerifications.value[moduleKeyForApp(module, expectedAppId)]
    return force || !previous || now - previous.checkedAt > AUTO_VERIFICATION_TTL_MS
  })
  await runWithConcurrency(modules, BACKGROUND_CONCURRENCY, async (module) => {
    await verifyModule(module, true, expectedAppId, expectedGeneration)
  })
}

async function checkModuleUpdate(
  module: ToolModuleDefinition,
  silent = false,
  expectedAppId = selectedAppId.value,
  expectedGeneration = selectionGeneration,
) {
  if (module.status !== 'available') return
  if (selectedAppId.value !== expectedAppId || selectionGeneration !== expectedGeneration) return

  const key = moduleKeyForApp(module, expectedAppId)
  if (updateBusyKeys.value.has(key)) return
  const config = module.config ?? {}
  const updateUrl = moduleUpdateSource(module)
  const configuredVersion = typeof config.version === 'string' ? config.version : null
  if (!updateUrl) {
    moduleUpdates.value[key] = {
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
    if (module.id === 'optiscaler') {
      const request = buildOptiScalerRequest(module)
      if (request) {
        const preview = await withTimeout(
          invoke<OptiScalerPreview>('preview_optiscaler', { request }),
          UPDATE_CHECK_TIMEOUT_MS,
          t('updateFailed'),
        )
        currentVersion = preview.installedVersion ?? request.version
      }
    }
    if (module.id === 'ofxr-framegen') {
      const request = buildOfxrRequest(module)
      if (request) {
        const preview = await withTimeout(
          invoke<OfxrPreview>('preview_ofxr', { request }),
          UPDATE_CHECK_TIMEOUT_MS,
          t('updateFailed'),
        )
        currentVersion = preview.installedVersion ?? request.version
      }
    }
    if (module.id === 'cheeky-foveated-dlss') {
      const request = buildCheekyRequest(module)
      if (request) {
        const preview = await withTimeout(
          invoke<CheekyFoveatedDlssPreview>('preview_cheeky_foveated_dlss', { request }),
          UPDATE_CHECK_TIMEOUT_MS,
          t('updateFailed'),
        )
        currentVersion = preview.installedVersion ?? request.version
      }
    }
    if (selectedAppId.value !== expectedAppId || selectionGeneration !== expectedGeneration) return

    const result = await withTimeout(invoke<ModuleUpdate>('check_module_update', {
      request: { currentVersion, updateUrl },
    }), UPDATE_CHECK_TIMEOUT_MS, t('updateFailed'))
    if (selectedAppId.value === expectedAppId && selectionGeneration === expectedGeneration) {
      moduleUpdates.value[key] = { ...result, checkedAt: Date.now() }
    }
  } catch (err) {
    if (selectedAppId.value === expectedAppId && selectionGeneration === expectedGeneration) {
      moduleUpdates.value[key] = {
        status: 'error',
        currentVersion,
        latestVersion: null,
        releaseUrl: null,
        checkedAt: Date.now(),
        detail: err instanceof Error ? err.message : String(err),
      }
      if (!silent) actionError.value = err instanceof Error ? err.message : String(err)
    }
  } finally {
    const nextBusyKeys = new Set(updateBusyKeys.value)
    nextBusyKeys.delete(key)
    updateBusyKeys.value = nextBusyKeys
  }
}

async function checkAvailableModuleUpdates(
  expectedAppId = selectedAppId.value,
  expectedGeneration = selectionGeneration,
) {
  const game = gameContextForAppId(expectedAppId)
  if (!game || selectedAppId.value !== expectedAppId || selectionGeneration !== expectedGeneration) return
  const now = Date.now()
  const modules = game.catalog.modules.filter((module) => {
    if (module.status !== 'available' || !moduleHasUpdateSource(module)) return false
    const previous = moduleUpdates.value[moduleKeyForApp(module, expectedAppId)]
    return !previous || now - previous.checkedAt > AUTO_UPDATE_TTL_MS
  })
  await runWithConcurrency(modules, BACKGROUND_CONCURRENCY, async (module) => {
    await checkModuleUpdate(module, true, expectedAppId, expectedGeneration)
  })
}

async function removeModule(module: ToolModuleDefinition) {
  if (selectedGameRunning.value) {
    actionError.value = t('gameRunningActionBlocked')
    return
  }

  const gameId = selectedGame.value?.catalog?.id
  const kind = moduleTransactionKind(module)
  if (!gameId || !kind || !activeModuleTransaction(module)) {
    actionError.value = t('noModuleTransaction')
    return
  }

  actionError.value = null
  success.value = null
  moduleBusy.value = true
  try {
    if (module.id === 'obs-vr') {
      const request = buildObsRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      await invoke<TransactionRecord>('uninstall_obs_vr', { request })
    } else if (module.id === 'optiscaler') {
      const request = buildOptiScalerRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      await invoke<TransactionRecord>('uninstall_optiscaler', { request })
    } else if (module.id === 'ofxr-framegen') {
      const request = buildOfxrRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      await invoke<TransactionRecord>('uninstall_ofxr', { request })
    } else if (module.id === 'cheeky-foveated-dlss') {
      const request = buildCheekyRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      await invoke<TransactionRecord>('uninstall_cheeky_foveated_dlss', { request })
    } else if (module.id === 'uevr') {
      const request = buildUevrRequest(module)
      if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
      await invoke<TransactionRecord>('uninstall_uevr', { request })
    } else {
      await invoke<TransactionRecord>('rollback_latest_module_transaction', { gameId, kind })
    }
    success.value = t('moduleRemoved', { module: moduleName(module) })
    await refreshTransactions()
    await inspectSelectedGame()
    await verifyModule(module, true)
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
  } finally {
    moduleBusy.value = false
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
      void verifyAvailableModules(expectedAppId, expectedGeneration)
      void checkAvailableModuleUpdates(expectedAppId, expectedGeneration)
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

watch(compatibilityReports, (value) => {
  try {
    window.localStorage.setItem('moddin-compatibility-reports', JSON.stringify(value))
  } catch {
    // Ignore unavailable storage, such as privacy-restricted browser contexts.
  }
}, { deep: true })

onMounted(async () => {
  appUnmounted = false
  loadCompatibilityReports()
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

      <div class="nav-section">
        <span class="nav-label">{{ t('navTools') }}</span>
        <GlobalTools />
      </div>

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
          <div>
            <strong>{{ t('libraryScanFailed') }}</strong>
            <p>{{ error }}</p>
          </div>
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
            <div v-if="!selectedGame" class="empty-state game-detail-empty">
              <AppIcon name="gamepad" :size="32" />
              <strong>{{ t('selectGameTitle') }}</strong>
              <span>{{ t('selectGameHint') }}</span>
            </div>

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
                  <AiTopbarMenu
                    :selected-app-id="selectedGame.catalog?.id ?? null"
                    :selected-game-name="selectedGame.catalog?.name ?? null"
                    :has-error="Boolean(error)"
                    @ask="aiTopbar.ask"
                    @recommend="aiTopbar.recommend"
                    @audit="aiTopbar.audit"
                    @diagnose="aiTopbar.diagnose"
                    @contribute="aiTopbar.contribute"
                  />
                  <button
                    v-if="primaryModule?.id === 'vr-launch'"
                    class="btn btn-primary btn-lg"
                    :class="{ 'is-loading': busyModuleId === primaryModule.id }"
                    type="button"
                    :disabled="moduleActionsBlocked(primaryModule)"
                    :title="blockedReason"
                    @click="configureModule(primaryModule)"
                  >
                    <AppIcon v-if="busyModuleId !== primaryModule.id" name="play" />
                    {{ moduleActionLabel(primaryModule) }}
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
                    <div v-if="availableModuleCategories.length > 1" class="segmented" role="group" :aria-label="t('modsTitle')">
                      <button type="button" :aria-pressed="activeModuleFilter === 'all'" @click="activeModuleFilter = 'all'">
                        {{ t('filterAllMods') }}
                      </button>
                      <button
                        v-for="category in availableModuleCategories"
                        :key="category"
                        type="button"
                        :aria-pressed="activeModuleFilter === category"
                        @click="activeModuleFilter = category"
                      >
                        {{ categoryLabel(category) }} <span class="count">{{ moduleCategoryCount(category) }}</span>
                      </button>
                    </div>
                  </div>

                  <section v-for="group in moduleGroups" :key="group.category" class="mod-group">
                    <h3 class="mod-group-title">{{ categoryLabel(group.category) }}</h3>
                    <div class="mod-grid">
                      <ModuleCard
                        v-for="module in group.modules"
                        :key="module.id"
                        :name="moduleName(module)"
                        :description="moduleDescription(module)"
                        :state="moduleCardState(module)"
                        :tag="moduleCardTag(module)"
                        :action-label="moduleActionLabel(module)"
                        :action-primary="moduleActionPrimary(module)"
                        :action-busy="busyModuleId === module.id"
                        :action-disabled="moduleActionsBlocked(module)"
                        :blocked-reason="blockedReason"
                        :verification="moduleVerification(module)"
                        :checked-at-label="moduleVerification(module) ? t('checkedAt', { date: formatTransactionDate(moduleVerification(module)!.checkedAt) }) : undefined"
                        :verify-busy="moduleVerificationBusy(module)"
                        :remove-label="moduleRemoveLabel(module)"
                        :update="moduleCardUpdate(module)"
                        @action="configureModule(module)"
                        @verify="verifyModule(module)"
                        @remove="removeModule(module)"
                        @check-update="checkModuleUpdate(module)"
                        @open-release="openRelease(moduleUpdate(module)?.releaseUrl ?? null)"
                      >
                        <template v-if="module.id === 'cheeky-foveated-dlss'" #details>
                          <section class="detail-block">
                            <h5 class="detail-title">{{ t('compatibilityStatus') }}</h5>
                            <p class="detail-copy">{{ cheekyResearchGuide(module).decision }}</p>
                            <p v-if="compatibilityReport(module)" class="detail-copy text-faint">{{ compatibilityStatusSummary(module) }}</p>
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

              <div v-else class="empty-state">
                <AppIcon name="info" :size="28" />
                <strong>{{ t('notCataloguedTitle') }}</strong>
                <span>{{ t('notCataloguedHint') }}</span>
              </div>
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
            </li>
          </ul>
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
          <option v-for="status in compatibilityStatuses" :key="status" :value="status">{{ compatibilityStatusLabel(status) }}</option>
        </select>
      </label>
      <label class="field">
        <span>{{ t('compatibilityTestNotes') }}</span>
        <textarea v-model="compatibilityDraftNote" class="textarea" rows="4" :placeholder="t('compatibilityTestNotesPlaceholder')" />
        <small>{{ t('compatibilityTestPrivacy') }}</small>
      </label>
      <template #footer>
        <button class="btn" type="button" @click="compatibilityDialog = null">{{ t('cancel') }}</button>
        <button class="btn btn-primary" type="button" @click="saveCompatibilityReport">{{ t('saveTestResult') }}</button>
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
    <AiAssistantTrigger
      :selected-game-id="selectedGame?.catalog?.id ?? null"
      :selected-game-name="selectedGame?.catalog?.name ?? null"
    />
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

@media (max-width: 960px) {
  .library-layout { display: flex; flex: none; flex-direction: column; }
  .library-layout :deep(.game-list) { flex: none; height: 300px; }
  .game-detail { flex: none; overflow: visible; padding: var(--moddin-space-4); }
  .game-hero { flex-direction: column; }
  .info-list > div { grid-template-columns: 1fr; gap: 2px; }
}
</style>
