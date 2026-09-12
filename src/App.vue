<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { findCatalogGameBySteamAppId, gameCatalog } from './services/catalog'
import type { InstalledGame } from './types/game'

const installedGames = ref<InstalledGame[]>([])
const loading = ref(true)
const error = ref<string | null>(null)
const search = ref('')
const selectedAppId = ref<string | null>(null)

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

onMounted(refreshGames)
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
        <button class="nav-item active">Library</button>
        <button class="nav-item" disabled>Transactions</button>
        <button class="nav-item" disabled>Settings</button>
      </nav>

      <div class="sidebar-footer">
        <span>v0.1.0 bootstrap</span>
        <small>{{ gameCatalog.length }} catalog games</small>
      </div>
    </aside>

    <main class="main-content">
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
                  <p>Modules are data-driven from the game catalog.</p>
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
                  <button class="module-button" :disabled="module.status !== 'available'">
                    {{ module.status === 'available' ? 'Configure' : 'Coming next' }}
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
    </main>
  </div>
</template>
