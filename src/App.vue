<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invokeDebug as invoke } from './debug'
import { findCatalogGameBySteamAppId, gameCatalog } from './services/catalog'
import { localeOptions } from './i18n'
import type { InstalledGame, ToolModuleDefinition } from './types/game'
import type { GameEnvironmentInspection } from './types/inspection'
import type { ObsVrPreview, ObsVrRequest } from './types/obs'
import type { OptiScalerPreview, OptiScalerRequest } from './types/optiscaler'
import type { TransactionRecord } from './types/transaction'
import type { VrIniPatch, VrLaunchPreview, VrLaunchRequest, VrLaunchResult, VrRecommendation } from './types/vr-launch'

type ViewName = 'library' | 'transactions'

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
const rollbackBusyId = ref<string | null>(null)
const obsDialog = ref<{ request: ObsVrRequest; preview: ObsVrPreview } | null>(null)
const optiScalerDialog = ref<{ request: OptiScalerRequest; preview: OptiScalerPreview } | null>(null)
const vrLaunchDialog = ref<{ request: VrLaunchRequest; preview: VrLaunchPreview } | null>(null)

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

function moduleName(module: ToolModuleDefinition) {
  if (module.id === 'vr-launch') return t('moduleVrLaunch')
  if (module.id === 'obs-vr') return t('moduleObsVr')
  if (module.id === 'optiscaler') return t('moduleOptiScaler')
  if (module.id === 'openxr') return t('moduleOpenXr')
  return module.name
}

function moduleDescription(module: ToolModuleDefinition) {
  if (module.id === 'vr-launch') return t('moduleVrLaunchDescription')
  if (module.id === 'obs-vr') return t('moduleObsVrDescription')
  if (module.id === 'optiscaler') return t('moduleOptiScalerDescription')
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

async function inspectSelectedGame() {
  gameInspection.value = null
  inspectionError.value = null

  const game = selectedGame.value
  if (!game?.catalog) return

  inspectionLoading.value = true
  try {
    gameInspection.value = await invoke<GameEnvironmentInspection>('inspect_game_environment', {
      installDir: game.installed.installDir,
      executable: game.catalog.executable,
    })
  } catch (err) {
    inspectionError.value = err instanceof Error ? err.message : String(err)
  } finally {
    inspectionLoading.value = false
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

async function configureModule(module: ToolModuleDefinition) {
  actionError.value = null
  success.value = null

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

    throw new Error(t('moduleNoAction', { module: module.id }))
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
  } finally {
    moduleBusy.value = false
  }
}

async function launchVrGame() {
  if (!vrLaunchDialog.value) return

  actionError.value = null
  success.value = null
  moduleBusy.value = true
  try {
    const request = vrLaunchDialog.value.request
    const result = await invoke<VrLaunchResult>('launch_vr_game', { request })
    vrLaunchDialog.value = null
    success.value = t('vrLaunchSuccess', { game: request.gameName, pid: result.processId })
    if (result.transaction) {
      success.value += ` ${t('vrConfigBackupCreated', { id: `${result.transaction.id.slice(0, 13)}…` })}`
      await refreshTransactions()
    }
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
  } finally {
    moduleBusy.value = false
  }
}

async function applyObsConfiguration() {
  if (!obsDialog.value) return

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
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
  } finally {
    moduleBusy.value = false
  }
}

async function applyOptiScaler() {
  if (!optiScalerDialog.value) return

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
    await Promise.all([refreshTransactions(), inspectSelectedGame()])
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
  } finally {
    moduleBusy.value = false
  }
}

async function rollback(transaction: TransactionRecord) {
  actionError.value = null
  success.value = null
  rollbackBusyId.value = transaction.id
  try {
    await invoke<TransactionRecord>('rollback_transaction', { id: transaction.id })
    success.value = t('rolledBack', { label: transaction.label })
    await Promise.all([refreshTransactions(), inspectSelectedGame()])
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

async function switchView(view: ViewName) {
  activeView.value = view
  actionError.value = null
  success.value = null
  if (view === 'transactions') await refreshTransactions()
}

watch(selectedAppId, () => {
  void inspectSelectedGame()
})

watch(locale, (value) => {
  try {
    window.localStorage.setItem('moddin-locale', value)
  } catch {
    // Ignore unavailable storage, such as privacy-restricted browser contexts.
  }
})

onMounted(async () => {
  await Promise.all([refreshGames(), refreshTransactions()])
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

                <div class="module-grid">
                  <article v-for="module in selectedGame.catalog.modules" :key="module.id" class="module-card">
                    <div class="module-topline">
                      <span class="category">{{ categoryLabel(module.category) }}</span>
                      <span class="module-state" :class="module.status">{{ t(module.status) }}</span>
                    </div>
                    <h4>{{ moduleName(module) }}</h4>
                    <p>{{ moduleDescription(module) }}</p>
                    <button
                      class="module-button"
                      :disabled="module.status !== 'available' || moduleBusy"
                      @click="configureModule(module)"
                    >
                      {{ module.status === 'available' ? (moduleBusy ? t('checking') : t('configure')) : t('comingNext') }}
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
                    <button class="secondary-button compact" :disabled="inspectionLoading" @click="inspectSelectedGame">
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
              :disabled="transaction.status !== 'applied' || rollbackBusyId === transaction.id"
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
            :disabled="!obsDialog.preview.canApply || moduleBusy"
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
            :disabled="!optiScalerDialog.preview.canApply || moduleBusy"
            @click="applyOptiScaler"
          >
            {{ moduleBusy ? t('applying') : t('applyWithBackup') }}
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
          <button class="primary-button" :disabled="!vrLaunchDialog.preview.canLaunch || moduleBusy" @click="launchVrGame">
            {{ moduleBusy ? t('launching') : t('launchVr') }}
          </button>
        </div>
      </section>
    </div>
  </div>
</template>
