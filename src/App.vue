<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invokeDebug as invoke } from './debug'
import { findCatalogGameBySteamAppId, gameCatalog } from './services/catalog'
import { localeOptions } from './i18n'
import type { InstalledGame, ToolModuleDefinition } from './types/game'
import type { GameEnvironmentInspection } from './types/inspection'
import type { ObsVrPreview, ObsVrRequest } from './types/obs'
import type { OptiScalerPreview, OptiScalerRequest } from './types/optiscaler'
import type { OfxrPreview, OfxrRequest, OfxrResult } from './types/ofxr'
import type { CheekyFoveatedDlssPreview, CheekyFoveatedDlssRequest, CheekyFoveatedDlssResult } from './types/cheeky'
import type { TransactionRecord } from './types/transaction'
import type { VrIniPatch, VrLaunchPreview, VrLaunchRequest, VrLaunchResult, VrRecommendation } from './types/vr-launch'
import type { ModuleVerification, ModuleVerificationCheck } from './types/module-verification'
import type { ModuleUpdate } from './types/module-update'
import type { CompatibilityReport, CompatibilityStatus } from './types/compatibility'

type ViewName = 'library' | 'transactions'
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
const selectedAppId = ref<string | null>(null)
const activeView = ref<ViewName>('library')
const moduleBusy = ref(false)
const verificationBusyKey = ref<string | null>(null)
const rollbackBusyId = ref<string | null>(null)
const moduleVerifications = ref<Record<string, ModuleVerification>>({})
const updateBusyKeys = ref(new Set<string>())
const moduleUpdates = ref<Record<string, ModuleUpdate>>({})
const compatibilityReports = ref<Record<string, CompatibilityReport>>({})
let gameStatePoll: number | null = null
const obsDialog = ref<{ request: ObsVrRequest; preview: ObsVrPreview } | null>(null)
const optiScalerDialog = ref<{ request: OptiScalerRequest; preview: OptiScalerPreview } | null>(null)
const ofxrDialog = ref<{ request: OfxrRequest; preview: OfxrPreview } | null>(null)
const cheekyDialog = ref<{ request: CheekyFoveatedDlssRequest; preview: CheekyFoveatedDlssPreview } | null>(null)
const cheekyGuideDialog = ref<ToolModuleDefinition | null>(null)
const compatibilityDialog = ref<{ module: ToolModuleDefinition; gameName: string; version: string } | null>(null)
const compatibilityDraftStatus = ref<CompatibilityStatus>('unverified')
const compatibilityDraftNote = ref('')
const vrLaunchDialog = ref<{ request: VrLaunchRequest; preview: VrLaunchPreview } | null>(null)

const compatibilityStatuses: CompatibilityStatus[] = [
  'unverified',
  'experimental',
  'proven',
  'risky',
  'not_working',
]

const { t, locale } = useI18n()

const supportedInstalledGames = computed(() =>
  installedGames.value.filter((game) => findCatalogGameBySteamAppId(game.appId)),
)

const filteredGames = computed(() => {
  const term = search.value.trim().toLowerCase()
  const ordered = [...installedGames.value].sort((a, b) => {
    const aSupported = Boolean(findCatalogGameBySteamAppId(a.appId))
    const bSupported = Boolean(findCatalogGameBySteamAppId(b.appId))
    if (aSupported !== bSupported) return aSupported ? -1 : 1
    return a.name.localeCompare(b.name)
  })

  if (!term) return ordered
  return ordered.filter((game) => game.name.toLowerCase().includes(term))
})

const selectedGame = computed(() => {
  const installed = installedGames.value.find((game) => game.appId === selectedAppId.value)
  if (!installed) return null
  return {
    installed,
    catalog: findCatalogGameBySteamAppId(installed.appId),
  }
})

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

function moduleName(module: ToolModuleDefinition) {
  if (module.id === 'vr-launch') return t('moduleVrLaunch')
  if (module.id === 'obs-vr') return t('moduleObsVr')
  if (module.id === 'optiscaler') return t('moduleOptiScaler')
  if (module.id === 'ofxr-framegen') return t('moduleOfxr')
  if (module.id === 'cheeky-foveated-dlss') return t('moduleCheeky')
  if (module.id === 'openxr') return t('moduleOpenXr')
  return module.name
}

function moduleDescription(module: ToolModuleDefinition) {
  if (module.id === 'vr-launch') return t('moduleVrLaunchDescription')
  if (module.id === 'obs-vr') return t('moduleObsVrDescription')
  if (module.id === 'optiscaler') return t('moduleOptiScalerDescription')
  if (module.id === 'ofxr-framegen') return t('moduleOfxrDescription')
  if (module.id === 'cheeky-foveated-dlss') return t('moduleCheekyDescription')
  if (module.id === 'openxr') return t('moduleOpenXrDescription')
  return module.description
}

function categoryLabel(category: ToolModuleDefinition['category']) {
  return t(`category${category.charAt(0).toUpperCase()}${category.slice(1)}`)
}

function statusLabel(status: TransactionRecord['status']) {
  if (status === 'applied') return t('statusApplied')
  if (status === 'rolled_back') return t('statusRolledBack')
  return status
}

function moduleKey(module: ToolModuleDefinition) {
  return `${selectedGame.value?.catalog?.id ?? selectedAppId.value ?? 'unknown'}:${module.id}`
}

function moduleVerification(module: ToolModuleDefinition) {
  return moduleVerifications.value[moduleKey(module)]
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

  return {
    state: isEldenRing ? 'prerequisite' : 'experimental',
    decision: t(isEldenRing ? 'cheekyGuideEldenDecision' : 'cheekyGuideCyberpunkDecision'),
    route: t(isEldenRing ? 'cheekyGuideEldenRoute' : 'cheekyGuideCyberpunkRoute'),
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
  if (locale.value === 'pt-BR') {
    if (gameId === 'cyberpunk-2077') return 'Cyberpunk 2077 foi citado pelo autor como não testado.'
    if (gameId === 'elden-ring') return 'Elden Ring não aparece na lista oficial de jogos testados.'
  } else if (locale.value === 'es') {
    if (gameId === 'cyberpunk-2077') return 'El autor indicó que Cyberpunk 2077 no había sido probado.'
    if (gameId === 'elden-ring') return 'Elden Ring no aparece en la lista oficial de juegos probados.'
  } else {
    if (gameId === 'cyberpunk-2077') return 'The author described Cyberpunk 2077 as untested.'
    if (gameId === 'elden-ring') return 'Elden Ring is not on the official tested-games list.'
  }
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

function moduleUpdateBusy(module: ToolModuleDefinition) {
  return updateBusyKeys.value.has(moduleKey(module))
}

function moduleUpdateLabel(status: ModuleUpdate['status']) {
  if (status === 'available') return t('updateAvailable')
  if (status === 'current') return t('updateCurrent')
  if (status === 'unavailable') return t('updatesNotConfigured')
  if (status === 'error') return t('updateCheckFailed')
  return t('updatesNotChecked')
}

function moduleUpdateSummary(module: ToolModuleDefinition) {
  const update = moduleUpdate(module)
  if (!update) return t('updatesNotChecked')
  if (update.status === 'available') {
    return t('updateAvailableSummary', { version: update.latestVersion ?? '?' })
  }
  if (update.status === 'current') {
    return t('updateCurrentSummary', { version: update.latestVersion ?? update.currentVersion ?? '?' })
  }
  if (update.status === 'unavailable') return t('updatesNotConfigured')
  if (update.status === 'error') return t('updateCheckFailed')
  if (update.status === 'unknown' && update.latestVersion) {
    return t('updateVersionUnknownSummary', { version: update.latestVersion })
  }
  return t('updateVersionUnknown')
}

function moduleActionsBlocked(module: ToolModuleDefinition) {
  return module.status !== 'available'
    || moduleBusy.value
    || inspectionLoading.value
    || selectedGameRunning.value
}

function verificationStatusLabel(status: ModuleVerification['status']) {
  if (status === 'installed') return t('verificationInstalled')
  if (status === 'ready') return t('verificationReady')
  if (status === 'attention') return t('verificationAttention')
  return t('verificationNotChecked')
}

function moduleActionLabel(module: ToolModuleDefinition) {
  const verification = moduleVerification(module)
  if (module.id === 'vr-launch') return t('reviewAndLaunch')
  if (verification?.status === 'installed') {
    return module.id === 'optiscaler' ? t('reinstall') : t('applyAgain')
  }
  return t('configure')
}

function moduleTransactionKind(module: ToolModuleDefinition) {
  if (module.id === 'obs-vr') return 'obs-vr'
  if (module.id === 'optiscaler') return 'optiscaler'
  if (module.id === 'ofxr-framegen') return 'ofxr-framegen'
  if (module.id === 'cheeky-foveated-dlss') return 'cheeky-foveated-dlss'
  if (module.id === 'vr-launch') return 'vr-launch'
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
) {
  moduleVerifications.value[moduleKey(module)] = {
    status,
    summary,
    checks,
    gameRunning,
    checkedAt: Date.now(),
    activeTransactionId: activeModuleTransaction(module)?.id ?? null,
  }
}

async function refreshGames() {
  loading.value = true
  error.value = null
  try {
    installedGames.value = await invoke<InstalledGame[]>('detect_steam_games')
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

async function inspectSelectedGame(silent = false) {
  if (!silent) {
    gameInspection.value = null
    inspectionError.value = null
  }

  const game = selectedGame.value
  if (!game?.catalog) return

  if (!silent) inspectionLoading.value = true
  const previousGameRunning = gameInspection.value?.gameRunning
  try {
    const nextInspection = await invoke<GameEnvironmentInspection>('inspect_game_environment', {
      installDir: game.installed.installDir,
      executable: game.catalog.executable,
    })
    gameInspection.value = nextInspection
    if (silent && previousGameRunning !== undefined && previousGameRunning !== nextInspection.gameRunning) {
      void verifyAvailableModules()
    }
  } catch (err) {
    if (!silent) inspectionError.value = err instanceof Error ? err.message : String(err)
  } finally {
    if (!silent) inspectionLoading.value = false
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
  if (locale.value === 'pt-BR') {
    const notes = ['Não use OptiScaler em sessões online com anti-cheat. Feche o jogo antes de instalar ou reverter arquivos.']
    if (gameId === 'elden-ring') {
      notes.push('No Elden Ring, o OptiScaler requer um mod que forneça entradas de upscaling/FG, como o ERSS-FG; o jogo vanilla não fornece essas entradas.')
    }
    return notes
  }

  if (locale.value === 'es') {
    const notes = ['No uses OptiScaler en sesiones online con anti-cheat. Cierra el juego antes de instalar o revertir archivos.']
    if (gameId === 'elden-ring') {
      notes.push('En Elden Ring, OptiScaler requiere un mod que proporcione entradas de escalado/FG, como ERSS-FG; el juego vanilla no ofrece esas entradas.')
    }
    return notes
  }

  const notes = ['Do not use OptiScaler in online sessions with anti-cheat. Close the game before installing or rolling files back.']
  if (gameId === 'elden-ring') {
    notes.push('On Elden Ring, OptiScaler requires a mod that provides upscaler/FG inputs, such as ERSS-FG; the vanilla game does not provide those inputs.')
  }
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
  if (locale.value === 'pt-BR') {
    return [
      'Requer ReShade 64-bit com suporte completo a add-ons instalado no mesmo jogo.',
      'Para VR em OpenXR, execute o CheekyOpenXRSetup.exe da mesma release uma vez; o Moddin não executa instaladores de terceiros automaticamente.',
      'Não use o add-on do ReShade junto com o plugin UEVR do Cheeky no mesmo jogo.',
      'Ative DLSS no jogo e ajuste a foveação pelo painel do ReShade depois da instalação.',
    ]
  }

  if (locale.value === 'es') {
    return [
      'Requiere ReShade de 64 bits con soporte completo de add-ons instalado en el mismo juego.',
      'Para VR en OpenXR, ejecuta una vez CheekyOpenXRSetup.exe de la misma release; Moddin no ejecuta instaladores de terceros automáticamente.',
      'No uses el add-on de ReShade junto con el plugin UEVR de Cheeky en el mismo juego.',
      'Activa DLSS en el juego y ajusta la foveación desde el panel de ReShade después de instalarlo.',
    ]
  }

  return [
    'Requires 64-bit ReShade with full add-on support installed for the same game.',
    'For OpenXR VR, run CheekyOpenXRSetup.exe from the matching release once; Moddin does not run third-party installers automatically.',
    'Do not use the Cheeky ReShade add-on together with the Cheeky UEVR plugin in the same game.',
    'Enable DLSS in the game and tune foveation through the ReShade panel after installation.',
  ]
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
    success.value = t('vrLaunchSuccess', { game: request.gameName, pid: result.processId })
    if (result.transaction) {
      success.value += ` ${t('vrConfigBackupCreated', { id: `${result.transaction.id.slice(0, 13)}…` })}`
    }
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
    success.value = t('configuredSuccess', {
      source: configuredSource,
      id: `${transaction.id.slice(0, 13)}…`,
    })
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
    success.value = t('configuredSuccess', {
      source: `OptiScaler ${request.version}`,
      id: `${transaction.id.slice(0, 13)}…`,
    })
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
      ? t('configuredSuccess', {
          source: `OFXR Bridge ${request.version}`,
          id: `${result.transaction.id.slice(0, 13)}â€¦`,
        })
      : t('ofxrConfiguredState')
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
    success.value = t('configuredSuccess', {
      source: `Cheeky Foveated DLSS ${request.version}`,
      id: `${transaction.id.slice(0, 13)}…`,
    })
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
    dateStyle: 'short',
    timeStyle: 'medium',
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

async function verifyModule(module: ToolModuleDefinition, silent = false) {
  if (module.status !== 'available') return

  const key = moduleKey(module)
  verificationBusyKey.value = key
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
      ], preview.gameRunning)
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
        check(t('checkVersion'), installed, preview.installed ? `${t('installedVersion')}: ${preview.installedVersion}` : undefined),
      ], preview.gameRunning)
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
      ], preview.gameRunning)
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
        check(t('checkVersion'), preview.installed && preview.installedVersion === request.version, preview.installedVersion ? `${t('installedVersion')}: ${preview.installedVersion}` : undefined),
        check(t('checkOfxrConfig'), preview.configured),
        check(t('checkOfxrTray'), preview.trayRunning),
        check(t('checkOfxrArmed'), preview.armed),
        check(t('checkGameClosed'), !preview.gameRunning),
        ], preview.gameRunning)
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
        check(t('checkVersion'), installed, preview.installed ? `${t('installedVersion')}: ${preview.installedVersion}` : undefined),
      ], preview.gameRunning)
    }

    if (!silent) success.value = t('verificationCompleted', { module: moduleName(module) })
  } catch (err) {
    if (!silent) actionError.value = err instanceof Error ? err.message : String(err)
  } finally {
    if (verificationBusyKey.value === key) verificationBusyKey.value = null
  }
}

async function verifyAvailableModules() {
  const modules = selectedGame.value?.catalog?.modules.filter((module) => module.status === 'available') ?? []
  await Promise.all(modules.map((module) => verifyModule(module, true)))
}

async function checkModuleUpdate(module: ToolModuleDefinition, silent = false) {
  if (module.status !== 'available') return

  const key = moduleKey(module)
  const busyKeys = new Set(updateBusyKeys.value)
  busyKeys.add(key)
  updateBusyKeys.value = busyKeys
  const config = module.config ?? {}
  const updateUrl = typeof config.updateUrl === 'string' ? config.updateUrl : null
  let currentVersion = typeof config.version === 'string' ? config.version : null

  try {
    if (module.id === 'optiscaler' && updateUrl) {
      const request = buildOptiScalerRequest(module)
      if (request) {
        const preview = await invoke<OptiScalerPreview>('preview_optiscaler', { request })
        currentVersion = preview.installedVersion ?? request.version
      }
    }
    if (module.id === 'ofxr-framegen' && updateUrl) {
      const request = buildOfxrRequest(module)
      if (request) {
        const preview = await invoke<OfxrPreview>('preview_ofxr', { request })
        currentVersion = preview.installedVersion ?? request.version
      }
    }
    if (module.id === 'cheeky-foveated-dlss' && updateUrl) {
      const request = buildCheekyRequest(module)
      if (request) {
        const preview = await invoke<CheekyFoveatedDlssPreview>('preview_cheeky_foveated_dlss', { request })
        currentVersion = preview.installedVersion ?? request.version
      }
    }

    const result = await invoke<ModuleUpdate>('check_module_update', {
      request: { currentVersion, updateUrl },
    })
    moduleUpdates.value[key] = { ...result, checkedAt: Date.now() }
  } catch (err) {
    moduleUpdates.value[key] = {
      status: 'error',
      currentVersion,
      latestVersion: null,
      releaseUrl: null,
      checkedAt: Date.now(),
      detail: err instanceof Error ? err.message : String(err),
    }
    if (!silent) actionError.value = err instanceof Error ? err.message : String(err)
  } finally {
    const nextBusyKeys = new Set(updateBusyKeys.value)
    nextBusyKeys.delete(key)
    updateBusyKeys.value = nextBusyKeys
  }
}

async function checkAvailableModuleUpdates() {
  const modules = selectedGame.value?.catalog?.modules.filter((module) => module.status === 'available') ?? []
  await Promise.all(modules.map((module) => checkModuleUpdate(module, true)))
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
  if (view === 'transactions') await refreshTransactions()
}

watch(selectedAppId, () => {
  void inspectSelectedGame()
  void verifyAvailableModules()
  void checkAvailableModuleUpdates()
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
  loadCompatibilityReports()
  await Promise.all([refreshGames(), refreshTransactions()])
  gameStatePoll = window.setInterval(() => {
    if (activeView.value === 'library' && selectedGame.value?.catalog) {
      void inspectSelectedGame(true)
    }
  }, 2000)
})

onUnmounted(() => {
  if (gameStatePoll !== null) window.clearInterval(gameStatePoll)
})
</script>

<template>
  <div class="app-shell" :lang="locale">
    <aside class="sidebar">
      <div class="brand">
        <div class="brand-mark">M</div>
        <div>
          <strong>Moddin</strong>
          <span>{{ t('brandTagline') }}</span>
        </div>
      </div>

      <nav class="nav-list">
        <button class="nav-item" :class="{ active: activeView === 'library' }" @click="switchView('library')">
          {{ t('library') }}
        </button>
        <button class="nav-item" :class="{ active: activeView === 'transactions' }" @click="switchView('transactions')">
          {{ t('transactions') }}
        </button>
        <button class="nav-item" disabled>{{ t('settings') }}</button>
      </nav>

      <div class="sidebar-footer">
        <label class="language-control">
          <span>{{ t('language') }}</span>
          <select v-model="locale">
            <option v-for="option in localeOptions" :key="option.value" :value="option.value">{{ option.label }}</option>
          </select>
        </label>
        <span>{{ t('bootstrap') }}</span>
        <small>{{ t('catalogGames', { count: gameCatalog.length }) }}</small>
      </div>
    </aside>

    <main class="main-content">
      <div v-if="success" class="success-banner">
        <strong>{{ t('done') }}</strong>
        <span>{{ success }}</span>
      </div>
      <div v-if="actionError" class="error-banner">
        <strong>{{ t('actionFailed') }}</strong>
        <span>{{ actionError }}</span>
      </div>

      <template v-if="activeView === 'library'">
        <header class="topbar">
          <div>
            <p class="eyebrow">{{ t('localLibrary') }}</p>
            <h1>{{ t('installedGames') }}</h1>
            <p class="subtle">
              {{ t('detectedSupported', { detected: installedGames.length, supported: supportedInstalledGames.length }) }}
            </p>
          </div>
          <button class="secondary-button" :disabled="loading" @click="refreshGames">
            {{ loading ? t('scanning') : t('rescanSteam') }}
          </button>
        </header>

        <div class="search-row">
          <input v-model="search" type="search" :placeholder="t('searchPlaceholder')" />
        </div>

        <div v-if="error" class="error-banner">
          <strong>{{ t('steamScanFailed') }}</strong>
          <span>{{ error }}</span>
        </div>

        <section class="workspace">
          <div class="game-list-panel">
            <div v-if="loading" class="empty-state">{{ t('scanningLibraries') }}</div>
            <div v-else-if="filteredGames.length === 0" class="empty-state">{{ t('noGamesFound') }}</div>

            <button
              v-for="game in filteredGames"
              :key="game.appId"
              class="game-row"
              :class="{ selected: selectedAppId === game.appId }"
              @click="selectedAppId = game.appId"
            >
              <div class="game-icon">{{ game.name.slice(0, 1).toUpperCase() }}</div>
              <div class="game-copy">
                <strong>{{ game.name }}</strong>
                <span>{{ game.installDir }}</span>
              </div>
              <span v-if="findCatalogGameBySteamAppId(game.appId)" class="status supported">{{ t('supported') }}</span>
              <span v-else class="status unsupported">{{ t('detected') }}</span>
            </button>
          </div>

          <div class="details-panel">
            <div v-if="!selectedGame" class="empty-state details-empty">
              {{ t('selectGame') }}
            </div>

            <template v-else>
              <div class="details-header">
                <div>
                  <p class="eyebrow">{{ t('steamApp', { id: selectedGame.installed.appId }) }}</p>
                  <h2>{{ selectedGame.installed.name }}</h2>
                  <p class="path">{{ selectedGame.installed.installDir }}</p>
                </div>
                <span v-if="selectedGame.catalog" class="status supported">{{ t('catalogMatch') }}</span>
                <span v-else class="status unsupported">{{ t('noRecipes') }}</span>
              </div>

              <template v-if="selectedGame.catalog">
                <div class="section-title">
                  <div>
                    <h3>{{ t('toolsRecipes') }}</h3>
                    <p>{{ t('previewChanges') }}</p>
                  </div>
                </div>

                <div v-if="selectedGameRunning" class="game-running-banner">
                  <strong>{{ t('gameRunningBanner') }}</strong>
                </div>

                <div class="module-grid">
                  <article v-for="module in selectedGame.catalog.modules" :key="module.id" class="module-card">
                    <div class="module-topline">
                      <span class="category">{{ categoryLabel(module.category) }}</span>
                      <div class="module-states">
                        <span
                          v-if="moduleVerification(module)?.status !== 'installed'"
                          class="module-state"
                          :class="module.status"
                        >
                          {{ t(module.status) }}
                        </span>
                        <span
                          v-if="moduleVerification(module)"
                          class="module-check-state"
                          :class="moduleVerification(module)?.status"
                        >
                          {{ verificationStatusLabel(moduleVerification(module)!.status) }}
                        </span>
                        <span
                          v-if="module.id === 'cheeky-foveated-dlss'"
                          class="compatibility-state"
                          :class="compatibilityStatus(module)"
                        >
                          {{ compatibilityStatusLabel(compatibilityStatus(module)) }}
                        </span>
                      </div>
                    </div>
                    <h4>{{ moduleName(module) }}</h4>
                    <p>{{ moduleDescription(module) }}</p>

                    <div v-if="module.id === 'cheeky-foveated-dlss'" class="compatibility-panel">
                      <div class="compatibility-heading">
                        <div>
                          <strong>{{ t('compatibilityStatus') }}</strong>
                          <small>{{ compatibilityStatusSummary(module) }}</small>
                        </div>
                        <span
                          class="research-state"
                          :class="cheekyResearchGuide(module).state"
                        >
                          {{ cheekyResearchStateLabel(cheekyResearchGuide(module).state) }}
                        </span>
                      </div>
                      <p class="compatibility-decision">{{ cheekyResearchGuide(module).decision }}</p>
                      <small class="compatibility-evidence">{{ cheekyCompatibilityEvidence() }}</small>
                      <div class="compatibility-actions">
                        <button class="secondary-button compact" @click="cheekyGuideDialog = module">
                          {{ t('viewCompatibilityGuide') }}
                        </button>
                        <button class="secondary-button compact" @click="openCompatibilityReport(module)">
                          {{ t('recordTest') }}
                        </button>
                      </div>
                    </div>

                    <div v-if="moduleVerification(module)" class="module-verification">
                      <div class="verification-heading">
                        <strong>{{ moduleVerification(module)?.summary }}</strong>
                        <small>{{ t('verifiedAt', { date: formatTransactionDate(moduleVerification(module)!.checkedAt) }) }}</small>
                      </div>
                      <ul class="verification-checklist">
                        <li
                          v-for="item in moduleVerification(module)?.checks"
                          :key="item.label"
                          :class="item.passed ? 'passed' : 'failed'"
                        >
                          <span aria-hidden="true">{{ item.passed ? '✓' : '!' }}</span>
                          <div>
                            <strong>{{ item.label }}</strong>
                            <small v-if="item.detail">{{ item.detail }}</small>
                          </div>
                        </li>
                      </ul>
                    </div>

                    <div v-if="module.status === 'available'" class="module-update-row">
                      <div class="module-update-copy">
                        <strong>{{ t('updates') }}</strong>
                        <small>{{ moduleUpdateSummary(module) }}</small>
                      </div>
                      <span
                        v-if="moduleUpdate(module)?.status === 'available'"
                        class="module-update-state available"
                      >
                        {{ moduleUpdateLabel(moduleUpdate(module)!.status) }}
                      </span>
                      <button
                        class="secondary-button compact"
                        :disabled="moduleUpdateBusy(module) || selectedGameRunning"
                        @click="checkModuleUpdate(module)"
                      >
                        {{ moduleUpdateBusy(module) ? t('checkingUpdates') : t('checkUpdates') }}
                      </button>
                    </div>

                    <div class="module-actions">
                      <button
                        class="secondary-button compact"
                        :disabled="module.status !== 'available' || verificationBusyKey === moduleKey(module) || selectedGameRunning"
                        @click="verifyModule(module)"
                      >
                        {{ verificationBusyKey === moduleKey(module) ? t('verifying') : t('verify') }}
                      </button>
                      <button
                        class="module-button"
                        :disabled="moduleActionsBlocked(module)"
                        :title="selectedGameRunning ? t('gameRunningActionBlocked') : undefined"
                        @click="configureModule(module)"
                      >
                        {{ module.status === 'available' ? (moduleBusy ? t('checking') : moduleActionLabel(module)) : t('comingNext') }}
                      </button>
                    </div>
                    <button
                      v-if="module.status === 'available' && activeModuleTransaction(module)"
                      class="text-button danger-action"
                      :disabled="moduleBusy || inspectionLoading || selectedGameRunning"
                      :title="selectedGameRunning ? t('gameRunningActionBlocked') : undefined"
                      @click="removeModule(module)"
                    >
                      {{ module.id === 'optiscaler'
                        ? t('uninstallOptiScaler')
                        : module.id === 'cheeky-foveated-dlss'
                          ? t('uninstallCheeky')
                          : t('removeConfiguration') }}
                    </button>
                  </article>
                </div>

                <div class="metadata-card">
                  <div>
                    <span>{{ t('executable') }}</span>
                    <strong>{{ selectedGame.catalog.executable }}</strong>
                  </div>
                  <div>
                    <span>{{ t('steamLibrary') }}</span>
                    <strong>{{ selectedGame.installed.libraryPath }}</strong>
                  </div>
                </div>

                <div class="environment-card">
                  <div class="environment-heading">
                    <div>
                      <span class="environment-label">{{ t('gameEnvironment') }}</span>
                      <h3>{{ t('injectionReadiness') }}</h3>
                    </div>
                    <button class="secondary-button compact" :disabled="inspectionLoading" @click="inspectSelectedGame()">
                      {{ inspectionLoading ? t('inspecting') : t('rescan') }}
                    </button>
                  </div>

                  <div v-if="inspectionLoading && !gameInspection" class="environment-message">{{ t('inspectingDirectory') }}</div>
                  <div v-else-if="inspectionError" class="environment-message danger">{{ inspectionError }}</div>
                  <template v-else-if="gameInspection">
                    <div class="environment-row">
                      <span>{{ t('executable') }}</span>
                      <strong :class="gameInspection.executableExists ? 'ok-text' : 'danger-text'">
                        {{ gameInspection.executableExists ? t('found') : t('missing') }}
                      </strong>
                    </div>
                    <code class="environment-path">{{ gameInspection.executablePath }}</code>

                    <div class="environment-row proxy-row">
                      <span>{{ t('proxyDlls') }}</span>
                      <strong :class="gameInspection.proxyDlls.length ? 'warning-text' : 'ok-text'">
                        {{ gameInspection.proxyDlls.length ? t('detectedCount', { count: gameInspection.proxyDlls.length }) : t('noneDetected') }}
                      </strong>
                    </div>

                    <div v-if="gameInspection.proxyDlls.length" class="dll-list">
                      <div v-for="dll in gameInspection.proxyDlls" :key="dll.path" class="dll-row">
                        <strong>{{ dll.name }}</strong>
                        <span>{{ formatFileSize(dll.sizeBytes) }}</span>
                        <code>{{ dll.path }}</code>
                      </div>
                    </div>
                    <p v-else class="environment-note">{{ t('noCommonProxy') }}</p>
                  </template>
                </div>
              </template>

              <div v-else class="unsupported-copy">
                <h3>{{ t('gameDetectedNotCataloged') }}</h3>
                <p>{{ t('gameDetectedDescription') }}</p>
              </div>
            </template>
          </div>
        </section>
      </template>

      <template v-else>
        <header class="topbar transactions-topbar">
          <div>
            <p class="eyebrow">{{ t('rollbackHistory') }}</p>
            <h1>{{ t('transactions') }}</h1>
            <p class="subtle">{{ t('everyDestructive') }}</p>
          </div>
          <button class="secondary-button" :disabled="transactionsLoading" @click="refreshTransactions">
            {{ transactionsLoading ? t('refreshing') : t('refresh') }}
          </button>
        </header>

        <section class="transactions-panel">
          <div v-if="transactionsLoading && transactions.length === 0" class="empty-state">{{ t('loadingTransactions') }}</div>
          <div v-else-if="transactions.length === 0" class="empty-state">{{ t('noTransactions') }}</div>

          <article v-for="transaction in transactions" :key="transaction.id" class="transaction-row">
            <div class="transaction-state" :class="transaction.status"></div>
            <div class="transaction-copy">
              <div class="transaction-title-row">
                <strong>{{ transaction.label }}</strong>
                <span class="status" :class="transaction.status === 'applied' ? 'supported' : 'unsupported'">
                  {{ statusLabel(transaction.status) }}
                </span>
              </div>
              <span>{{ formatTransactionDate(transaction.createdAt) }} · {{ transaction.kind }} · {{ transaction.gameId }}</span>
              <code>{{ transaction.targetPath }}</code>
            </div>
            <button
              class="secondary-button"
              :disabled="transaction.status !== 'applied'
                || rollbackBusyId === transaction.id
                || inspectionLoading
                || (selectedGameRunning && transaction.gameId === selectedGame?.catalog?.id)"
              :title="selectedGameRunning && transaction.gameId === selectedGame?.catalog?.id ? t('gameRunningActionBlocked') : undefined"
              @click="rollback(transaction)"
            >
              {{ rollbackBusyId === transaction.id ? t('restoring') : t('undo') }}
            </button>
          </article>
        </section>
      </template>
    </main>

    <div v-if="obsDialog" class="modal-backdrop" @click.self="obsDialog = null">
      <section class="modal-card">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">{{ t('preview') }}</p>
            <h2>{{ obsDialog.request.sourceName }}</h2>
          </div>
          <button class="icon-button" :aria-label="t('close')" @click="obsDialog = null">×</button>
        </div>

        <div class="preview-summary">
          <div>
            <span>{{ t('collection') }}</span>
            <strong>{{ obsDialog.preview.collectionName ?? t('notFound') }}</strong>
          </div>
          <div>
            <span>{{ t('scene') }}</span>
            <strong>{{ obsDialog.request.sceneName }}</strong>
          </div>
          <div>
            <span>{{ t('executable') }}</span>
            <strong>{{ obsDialog.request.executableName }}</strong>
          </div>
        </div>

        <div class="preview-block">
          <h3>{{ t('changes') }}</h3>
          <ul v-if="obsDialog.preview.changes.length">
            <li v-for="change in obsDialog.preview.changes" :key="change">{{ change }}</li>
          </ul>
          <p v-else>{{ t('noSafePlan') }}</p>
        </div>

        <div v-if="obsDialog.preview.warnings.length" class="preview-block warnings">
          <h3>{{ t('notes') }}</h3>
          <ul>
            <li v-for="warning in obsDialog.preview.warnings" :key="warning">{{ warning }}</li>
          </ul>
        </div>

        <p v-if="obsDialog.preview.collectionFile" class="path modal-path">{{ obsDialog.preview.collectionFile }}</p>

        <div class="modal-actions">
          <button class="secondary-button" @click="obsDialog = null">{{ t('cancel') }}</button>
          <button
            class="primary-button"
            :disabled="!obsDialog.preview.canApply || moduleBusy || inspectionLoading || selectedGameRunning"
            @click="applyObsConfiguration"
          >
            {{ moduleBusy ? t('applying') : t('applyWithBackup') }}
          </button>
        </div>
      </section>
    </div>

    <div v-if="optiScalerDialog" class="modal-backdrop" @click.self="optiScalerDialog = null">
      <section class="modal-card optiscaler-modal">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">{{ t('preview') }} · OPTISCALER</p>
            <h2>OptiScaler {{ optiScalerDialog.request.version }}</h2>
          </div>
          <button class="icon-button" :aria-label="t('close')" @click="optiScalerDialog = null">×</button>
        </div>

        <div class="preview-summary">
          <div>
            <span>Version</span>
            <strong>{{ optiScalerDialog.request.version }}</strong>
          </div>
          <div>
            <span>Proxy DLL</span>
            <strong>{{ optiScalerDialog.preview.selectedProxy ?? t('notFound') }}</strong>
          </div>
          <div>
            <span>Status</span>
            <strong>{{ optiScalerDialog.preview.installed ? `Installed ${optiScalerDialog.preview.installedVersion ?? ''}` : 'Not installed' }}</strong>
          </div>
        </div>

        <div v-if="optiScalerDialog.preview.conflicts.length" class="preview-block warnings">
          <h3>Proxy DLL conflicts</h3>
          <ul>
            <li v-for="conflict in optiScalerDialog.preview.conflicts" :key="conflict.path">
              {{ conflict.name }} · {{ formatFileSize(conflict.sizeBytes) }} · {{ conflict.path }}
            </li>
          </ul>
        </div>

        <div class="preview-block">
          <h3>{{ t('changes') }}</h3>
          <ul v-if="optiScalerDialog.preview.changes.length">
            <li v-for="change in optiScalerDialog.preview.changes" :key="change">{{ change }}</li>
          </ul>
          <p v-else>{{ t('noSafePlan') }}</p>
        </div>

        <div v-if="optiScalerDialog.preview.warnings.length" class="preview-block warnings">
          <h3>{{ t('notes') }}</h3>
          <ul>
            <li v-for="warning in optiScalerDialog.preview.warnings" :key="warning">{{ warning }}</li>
          </ul>
        </div>

        <p class="path modal-path">{{ optiScalerDialog.preview.executableDirectory }}</p>

        <div class="modal-actions">
          <button class="secondary-button" @click="optiScalerDialog = null">{{ t('cancel') }}</button>
          <button
            class="primary-button"
            :disabled="!optiScalerDialog.preview.canApply || moduleBusy || inspectionLoading || selectedGameRunning"
            @click="applyOptiScaler"
          >
            {{ moduleBusy ? t('applying') : t('applyWithBackup') }}
          </button>
        </div>
      </section>
    </div>

    <div v-if="cheekyDialog" class="modal-backdrop" @click.self="cheekyDialog = null">
      <section class="modal-card">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">{{ t('preview') }} · CHEEKY FOVEATED DLSS</p>
            <h2>Cheeky Foveated DLSS {{ cheekyDialog.request.version }}</h2>
          </div>
          <button class="icon-button" :aria-label="t('close')" @click="cheekyDialog = null">×</button>
        </div>

        <div class="preview-summary">
          <div>
            <span>{{ t('cheekyVersion') }}</span>
            <strong>{{ cheekyDialog.request.version }}</strong>
          </div>
          <div>
            <span>{{ t('cheekyIntegration') }}</span>
            <strong>{{ t('cheekyReshadeAddon') }}</strong>
          </div>
          <div>
            <span>{{ t('cheekyAddon') }}</span>
            <strong>{{ cheekyDialog.request.addonFile }}</strong>
          </div>
        </div>

        <div class="preview-block">
          <h3>{{ t('changes') }}</h3>
          <ul v-if="cheekyDialog.preview.changes.length">
            <li v-for="change in cheekyDialog.preview.changes" :key="change">{{ change }}</li>
          </ul>
          <p v-else>{{ t('noSafePlan') }}</p>
        </div>

        <div v-if="cheekyDialog.preview.warnings.length" class="preview-block warnings">
          <h3>{{ t('notes') }}</h3>
          <ul>
            <li v-for="warning in cheekyDialog.preview.warnings" :key="warning">{{ warning }}</li>
          </ul>
        </div>

        <p class="path modal-path">{{ cheekyDialog.preview.addonPath }}</p>

        <div class="modal-actions">
          <button class="secondary-button" @click="cheekyDialog = null">{{ t('cancel') }}</button>
          <button
            class="primary-button"
            :disabled="!cheekyDialog.preview.canApply || moduleBusy || inspectionLoading || selectedGameRunning"
            @click="applyCheeky"
          >
            {{ moduleBusy ? t('applying') : t('applyWithBackup') }}
          </button>
        </div>
      </section>
    </div>

    <div v-if="cheekyGuideDialog" class="modal-backdrop" @click.self="cheekyGuideDialog = null">
      <section class="modal-card compatibility-guide-card">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">{{ t('cheekyGuideResearchBased') }} · CHEEKY FOVEATED DLSS</p>
            <h2>{{ t('compatibilityGuide') }}</h2>
          </div>
          <button class="icon-button" :aria-label="t('close')" @click="cheekyGuideDialog = null">×</button>
        </div>

        <div class="guide-decision-card">
          <span
            class="research-state"
            :class="cheekyResearchGuide(cheekyGuideDialog).state"
          >
            {{ cheekyResearchStateLabel(cheekyResearchGuide(cheekyGuideDialog).state) }}
          </span>
          <div>
            <span>{{ t('cheekyGuideDecision') }}</span>
            <strong>{{ cheekyResearchGuide(cheekyGuideDialog).decision }}</strong>
          </div>
        </div>

        <div class="preview-block">
          <h3>{{ t('cheekyGuideRoute') }}</h3>
          <p>{{ cheekyResearchGuide(cheekyGuideDialog).route }}</p>
        </div>

        <div class="preview-block guide-list">
          <h3>{{ t('cheekyGuidePrerequisites') }}</h3>
          <ul>
            <li v-for="item in cheekyResearchGuide(cheekyGuideDialog).prerequisites" :key="item">{{ item }}</li>
          </ul>
        </div>

        <div class="preview-block guide-list">
          <h3>{{ t('cheekyGuideTestChecklist') }}</h3>
          <ol>
            <li v-for="item in cheekyResearchGuide(cheekyGuideDialog).validation" :key="item">{{ item }}</li>
          </ol>
        </div>

        <div class="preview-block warnings">
          <h3>{{ t('cheekyGuideRisks') }}</h3>
          <p>{{ cheekyResearchGuide(cheekyGuideDialog).risk }}</p>
        </div>

        <div class="modal-actions">
          <button class="secondary-button" @click="cheekyGuideDialog = null">{{ t('close') }}</button>
          <button class="primary-button" @click="openCompatibilityReport(cheekyGuideDialog)">
            {{ t('recordTest') }}
          </button>
        </div>
      </section>
    </div>

    <div v-if="compatibilityDialog" class="modal-backdrop" @click.self="compatibilityDialog = null">
      <section class="modal-card">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">{{ t('compatibilityTestTitle') }} · CHEEKY FOVEATED DLSS</p>
            <h2>{{ compatibilityDialog.gameName }}</h2>
          </div>
          <button class="icon-button" :aria-label="t('close')" @click="compatibilityDialog = null">×</button>
        </div>

        <div class="preview-summary">
          <div>
            <span>{{ t('cheekyVersion') }}</span>
            <strong>{{ compatibilityDialog.version }}</strong>
          </div>
          <div>
            <span>{{ t('compatibilityTestStatus') }}</span>
            <strong>{{ compatibilityStatusLabel(compatibilityDraftStatus) }}</strong>
          </div>
        </div>

        <div class="compatibility-form">
          <label>
            <span>{{ t('compatibilityTestStatus') }}</span>
            <select v-model="compatibilityDraftStatus">
              <option value="unverified">{{ t('compatibilityUnverified') }}</option>
              <option value="experimental">{{ t('compatibilityExperimental') }}</option>
              <option value="proven">{{ t('compatibilityProven') }}</option>
              <option value="risky">{{ t('compatibilityRisky') }}</option>
              <option value="not_working">{{ t('compatibilityNotWorking') }}</option>
            </select>
          </label>
          <label>
            <span>{{ t('compatibilityTestNotes') }}</span>
            <textarea v-model="compatibilityDraftNote" rows="4" :placeholder="t('compatibilityTestNotesPlaceholder')" />
          </label>
        </div>

        <div class="preview-block warnings">
          <h3>{{ t('notes') }}</h3>
          <p>{{ cheekyCompatibilityEvidence() }}</p>
        </div>

        <div class="modal-actions">
          <button class="secondary-button" @click="compatibilityDialog = null">{{ t('cancel') }}</button>
          <button class="primary-button" @click="saveCompatibilityReport">{{ t('saveTestResult') }}</button>
        </div>
      </section>
    </div>

    <div v-if="ofxrDialog" class="modal-backdrop" @click.self="ofxrDialog = null">
      <section class="modal-card">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">{{ t('preview') }} Â· OFXR FRAMEGEN</p>
            <h2>OFXR Bridge {{ ofxrDialog.request.version }}</h2>
          </div>
          <button class="icon-button" :aria-label="t('close')" @click="ofxrDialog = null">Ã—</button>
        </div>

        <div class="preview-summary">
          <div>
            <span>{{ t('ofxrVersion') }}</span>
            <strong>{{ ofxrDialog.request.version }} (V{{ ofxrDialog.request.implementationVersion }})</strong>
          </div>
          <div>
            <span>{{ t('ofxrBackend') }}</span>
            <strong>{{ ofxrDialog.request.backend }}</strong>
          </div>
          <div>
            <span>{{ t('ofxrTray') }}</span>
            <strong>{{ ofxrDialog.preview.armed ? t('ofxrConfiguredState') : t('ofxrNotConfiguredState') }}</strong>
          </div>
        </div>

        <div class="preview-block">
          <h3>{{ t('changes') }}</h3>
          <ul v-if="ofxrDialog.preview.changes.length">
            <li v-for="change in ofxrDialog.preview.changes" :key="change">{{ change }}</li>
          </ul>
          <p v-else>{{ t('ofxrReadyForLaunch') }}</p>
        </div>

        <div v-if="ofxrDialog.preview.warnings.length" class="preview-block warnings">
          <h3>{{ t('notes') }}</h3>
          <ul>
            <li v-for="warning in ofxrDialog.preview.warnings" :key="warning">{{ warning }}</li>
          </ul>
        </div>

        <p class="path modal-path">{{ ofxrDialog.preview.installDirectory }}</p>

        <div class="modal-actions">
          <button class="secondary-button" @click="ofxrDialog = null">{{ t('cancel') }}</button>
          <button
            class="primary-button"
            :disabled="!ofxrDialog.preview.canApply || moduleBusy || inspectionLoading || selectedGameRunning"
            @click="applyOfxr"
          >
            {{ moduleBusy ? t('applying') : t('ofxrInstallAndArm') }}
          </button>
        </div>
      </section>
    </div>

    <div v-if="vrLaunchDialog" class="modal-backdrop" @click.self="vrLaunchDialog = null">
      <section class="modal-card">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">{{ t('vrLaunchPreview') }}</p>
            <h2>{{ vrLaunchDialog.request.gameName }}</h2>
          </div>
          <button class="icon-button" :aria-label="t('close')" @click="vrLaunchDialog = null">×</button>
        </div>

        <div class="preview-summary">
          <div>
            <span>{{ t('vrRuntime') }}</span>
            <strong>{{ vrLaunchDialog.preview.activeOpenXrRuntime ?? t('notDetected') }}</strong>
          </div>
          <div>
            <span>{{ t('executable') }}</span>
            <strong>{{ vrLaunchDialog.request.executable }}</strong>
          </div>
          <div>
            <span>{{ t('launchArguments') }}</span>
            <strong>{{ vrLaunchDialog.request.arguments.join(' ') || t('none') }}</strong>
          </div>
        </div>

        <div class="preview-block">
          <h3>{{ t('recommendedGraphics') }}</h3>
          <ul>
            <li v-for="recommendation in vrLaunchDialog.request.recommendations" :key="`${recommendation.label}-${recommendation.value}`">
              <strong>{{ recommendation.label }}:</strong> {{ recommendation.value }}
            </li>
          </ul>
        </div>

        <div v-if="ofxrLaunchRequest" class="preview-block ofxr-launch-block">
          <h3>{{ t('ofxrBeforeLaunch') }}</h3>
          <p>{{ ofxrReadyForLaunch ? t('ofxrReadyForLaunch') : t('ofxrWillPrepareBeforeLaunch') }}</p>
        </div>

        <div v-if="vrLaunchDialog.preview.settings.length" class="preview-block">
          <h3>{{ t('settingsToApply') }}</h3>
          <ul>
            <li v-for="setting in vrLaunchDialog.preview.settings" :key="`${setting.section}-${setting.key}`">
              <code>{{ setting.section }}.{{ setting.key }}</code>:
              {{ setting.currentValue ?? t('notConfigured') }} → {{ setting.value }}
            </li>
          </ul>
        </div>

        <div class="preview-block">
          <h3>{{ t('changes') }}</h3>
          <ul>
            <li v-for="change in vrLaunchDialog.preview.changes" :key="change">{{ change }}</li>
          </ul>
        </div>

        <div v-if="vrLaunchDialog.preview.warnings.length" class="preview-block warnings">
          <h3>{{ t('notes') }}</h3>
          <ul>
            <li v-for="warning in vrLaunchDialog.preview.warnings" :key="warning">{{ warning }}</li>
          </ul>
        </div>

        <p v-if="vrLaunchDialog.preview.configPath" class="path modal-path">{{ vrLaunchDialog.preview.configPath }}</p>

        <div class="modal-actions">
          <button class="secondary-button" @click="vrLaunchDialog = null">{{ t('cancel') }}</button>
          <button class="primary-button" :disabled="!vrLaunchDialog.preview.canLaunch || moduleBusy || inspectionLoading || selectedGameRunning" @click="launchVrGame">
            {{ moduleBusy ? t('launching') : (ofxrReadyForLaunch ? t('launchVr') : t('activateAndLaunch')) }}
          </button>
        </div>
      </section>
    </div>
  </div>
</template>
