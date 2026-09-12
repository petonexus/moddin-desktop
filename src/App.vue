<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { findCatalogGameBySteamAppId, gameCatalog } from './services/catalog'
import type { InstalledGame, ToolModuleDefinition } from './types/game'
import type { ObsVrPreview, ObsVrRequest } from './types/obs'
import type { TransactionRecord } from './types/transaction'

type ViewName = 'library' | 'transactions'

const installedGames = ref<InstalledGame[]>([])
const transactions = ref<TransactionRecord[]>([])
const loading = ref(true)
const transactionsLoading = ref(false)
const error = ref<string | null>(null)
const actionError = ref<string | null>(null)
const success = ref<string | null>(null)
const search = ref('')
const selectedAppId = ref<string | null>(null)
const activeView = ref<ViewName>('library')
const moduleBusy = ref(false)
const rollbackBusyId = ref<string | null>(null)
const obsDialog = ref<{ request: ObsVrRequest; preview: ObsVrPreview } | null>(null)

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

async function refreshGames() {
  loading.value = true
  error.value = null
  try {
    installedGames.value = await invoke<InstalledGame[]>('detect_steam_games')
    if (!selectedAppId.value) {
      selectedAppId.value = supportedInstalledGames.value[0]?.appId ?? installedGames.value[0]?.appId ?? null
    }
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    loading.value = false
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
  const executableName = config.executableName || selectedGame.value.catalog.executable.split(/[\\/]/).pop()
  if (!executableName) return null

  return {
    gameId: selectedGame.value.catalog.id,
    gameName: selectedGame.value.catalog.name,
    collectionName: config.collectionName || 'Sem nome',
    sceneName: config.sceneName || 'vr',
    sourceName: config.sourceName || `${selectedGame.value.catalog.name} VR`,
    executableName,
  }
}

async function configureModule(module: ToolModuleDefinition) {
  actionError.value = null
  success.value = null

  const request = buildObsRequest(module)
  if (!request) {
    actionError.value = `Module '${module.id}' does not have an executable action yet.`
    return
  }

  moduleBusy.value = true
  try {
    const preview = await invoke<ObsVrPreview>('preview_obs_vr', { request })
    obsDialog.value = { request, preview }
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
    const transaction = await invoke<TransactionRecord>('configure_obs_vr', {
      request: obsDialog.value.request,
    })
    success.value = `${obsDialog.value.request.sourceName} configured. Backup transaction ${transaction.id.slice(0, 13)}… created.`
    obsDialog.value = null
    await refreshTransactions()
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
    success.value = `Rolled back: ${transaction.label}`
    await refreshTransactions()
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
  } finally {
    rollbackBusyId.value = null
  }
}

function formatTransactionDate(timestamp: number) {
  return new Intl.DateTimeFormat('pt-BR', {
    dateStyle: 'short',
    timeStyle: 'medium',
  }).format(new Date(timestamp))
}

async function switchView(view: ViewName) {
  activeView.value = view
  actionError.value = null
  success.value = null
  if (view === 'transactions') await refreshTransactions()
}

onMounted(async () => {
  await Promise.all([refreshGames(), refreshTransactions()])
})
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <div class="brand">
        <div class="brand-mark">M</div>
        <div>
          <strong>Moddin</strong>
          <span>Game tooling manager</span>
        </div>
      </div>

      <nav class="nav-list">
        <button class="nav-item" :class="{ active: activeView === 'library' }" @click="switchView('library')">Library</button>
        <button class="nav-item" :class="{ active: activeView === 'transactions' }" @click="switchView('transactions')">
          Transactions
        </button>
        <button class="nav-item" disabled>Settings</button>
      </nav>

      <div class="sidebar-footer">
        <span>v0.1.0 bootstrap</span>
        <small>{{ gameCatalog.length }} catalog games</small>
      </div>
    </aside>

    <main class="main-content">
      <div v-if="success" class="success-banner">
        <strong>Done.</strong>
        <span>{{ success }}</span>
      </div>
      <div v-if="actionError" class="error-banner">
        <strong>Action failed.</strong>
        <span>{{ actionError }}</span>
      </div>

      <template v-if="activeView === 'library'">
        <header class="topbar">
          <div>
            <p class="eyebrow">LOCAL LIBRARY</p>
            <h1>Installed games</h1>
            <p class="subtle">
              {{ installedGames.length }} detected · {{ supportedInstalledGames.length }} supported by Moddin
            </p>
          </div>
          <button class="secondary-button" :disabled="loading" @click="refreshGames">
            {{ loading ? 'Scanning…' : 'Rescan Steam' }}
          </button>
        </header>

        <div class="search-row">
          <input v-model="search" type="search" placeholder="Search installed games…" />
        </div>

        <div v-if="error" class="error-banner">
          <strong>Steam scan failed.</strong>
          <span>{{ error }}</span>
        </div>

        <section class="workspace">
          <div class="game-list-panel">
            <div v-if="loading" class="empty-state">Scanning Steam libraries…</div>
            <div v-else-if="filteredGames.length === 0" class="empty-state">No games found.</div>

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
              <span v-if="findCatalogGameBySteamAppId(game.appId)" class="status supported">Supported</span>
              <span v-else class="status unsupported">Detected</span>
            </button>
          </div>

          <div class="details-panel">
            <div v-if="!selectedGame" class="empty-state details-empty">
              Select a game to inspect available recipes.
            </div>

            <template v-else>
              <div class="details-header">
                <div>
                  <p class="eyebrow">STEAM APP {{ selectedGame.installed.appId }}</p>
                  <h2>{{ selectedGame.installed.name }}</h2>
                  <p class="path">{{ selectedGame.installed.installDir }}</p>
                </div>
                <span v-if="selectedGame.catalog" class="status supported">Catalog match</span>
                <span v-else class="status unsupported">No recipes yet</span>
              </div>

              <template v-if="selectedGame.catalog">
                <div class="section-title">
                  <div>
                    <h3>Tools & recipes</h3>
                    <p>Preview changes first. Moddin creates a rollback transaction before touching files.</p>
                  </div>
                </div>

                <div class="module-grid">
                  <article v-for="module in selectedGame.catalog.modules" :key="module.id" class="module-card">
                    <div class="module-topline">
                      <span class="category">{{ module.category }}</span>
                      <span class="module-state" :class="module.status">{{ module.status }}</span>
                    </div>
                    <h4>{{ module.name }}</h4>
                    <p>{{ module.description }}</p>
                    <button
                      class="module-button"
                      :disabled="module.status !== 'available' || moduleBusy"
                      @click="configureModule(module)"
                    >
                      {{ module.status === 'available' ? (moduleBusy ? 'Checking…' : 'Configure') : 'Coming next' }}
                    </button>
                  </article>
                </div>

                <div class="metadata-card">
                  <div>
                    <span>Executable</span>
                    <strong>{{ selectedGame.catalog.executable }}</strong>
                  </div>
                  <div>
                    <span>Steam library</span>
                    <strong>{{ selectedGame.installed.libraryPath }}</strong>
                  </div>
                </div>
              </template>

              <div v-else class="unsupported-copy">
                <h3>Game detected, but not cataloged yet.</h3>
                <p>
                  Moddin already knows where this game lives. Adding support later should only require a catalog recipe,
                  not hard-coded UI logic.
                </p>
              </div>
            </template>
          </div>
        </section>
      </template>

      <template v-else>
        <header class="topbar transactions-topbar">
          <div>
            <p class="eyebrow">ROLLBACK HISTORY</p>
            <h1>Transactions</h1>
            <p class="subtle">Every destructive configuration starts with a backup transaction.</p>
          </div>
          <button class="secondary-button" :disabled="transactionsLoading" @click="refreshTransactions">
            {{ transactionsLoading ? 'Refreshing…' : 'Refresh' }}
          </button>
        </header>

        <section class="transactions-panel">
          <div v-if="transactionsLoading && transactions.length === 0" class="empty-state">Loading transactions…</div>
          <div v-else-if="transactions.length === 0" class="empty-state">No transactions yet.</div>

          <article v-for="transaction in transactions" :key="transaction.id" class="transaction-row">
            <div class="transaction-state" :class="transaction.status"></div>
            <div class="transaction-copy">
              <div class="transaction-title-row">
                <strong>{{ transaction.label }}</strong>
                <span class="status" :class="transaction.status === 'applied' ? 'supported' : 'unsupported'">
                  {{ transaction.status }}
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
              {{ rollbackBusyId === transaction.id ? 'Restoring…' : 'Undo' }}
            </button>
          </article>
        </section>
      </template>
    </main>

    <div v-if="obsDialog" class="modal-backdrop" @click.self="obsDialog = null">
      <section class="modal-card">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">PREVIEW</p>
            <h2>{{ obsDialog.request.sourceName }}</h2>
          </div>
          <button class="icon-button" @click="obsDialog = null">×</button>
        </div>

        <div class="preview-summary">
          <div>
            <span>Collection</span>
            <strong>{{ obsDialog.preview.collectionName ?? 'Not found' }}</strong>
          </div>
          <div>
            <span>Scene</span>
            <strong>{{ obsDialog.request.sceneName }}</strong>
          </div>
          <div>
            <span>Executable</span>
            <strong>{{ obsDialog.request.executableName }}</strong>
          </div>
        </div>

        <div class="preview-block">
          <h3>Changes</h3>
          <ul v-if="obsDialog.preview.changes.length">
            <li v-for="change in obsDialog.preview.changes" :key="change">{{ change }}</li>
          </ul>
          <p v-else>No safe configuration plan could be created.</p>
        </div>

        <div v-if="obsDialog.preview.warnings.length" class="preview-block warnings">
          <h3>Notes</h3>
          <ul>
            <li v-for="warning in obsDialog.preview.warnings" :key="warning">{{ warning }}</li>
          </ul>
        </div>

        <p v-if="obsDialog.preview.collectionFile" class="path modal-path">{{ obsDialog.preview.collectionFile }}</p>

        <div class="modal-actions">
          <button class="secondary-button" @click="obsDialog = null">Cancel</button>
          <button
            class="primary-button"
            :disabled="!obsDialog.preview.canApply || moduleBusy"
            @click="applyObsConfiguration"
          >
            {{ moduleBusy ? 'Applying…' : 'Apply with backup' }}
          </button>
        </div>
      </section>
    </div>
  </div>
</template>
