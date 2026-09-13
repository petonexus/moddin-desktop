<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invokeDebug as invoke } from '../debug'
import { findCatalogGameBySteamAppId } from '../services/catalog'
import type { InstalledGame } from '../types/game'
import type { OpenXrRuntimeInfo, OpenXrState } from '../types/openxr'

const { locale } = useI18n()

const open = ref(false)
const loading = ref(false)
const busyAction = ref<string | null>(null)
const error = ref<string | null>(null)
const state = ref<OpenXrState | null>(null)
const installedGames = ref<InstalledGame[]>([])
const selectedGameId = ref<string>('')

const messages = {
  'pt-BR': {
    button: 'OpenXR',
    title: 'Gerenciador OpenXR',
    subtitle: 'Runtime do Windows e override por jogo',
    close: 'Fechar',
    refresh: 'Atualizar',
    selectGame: 'Jogo para override',
    systemOnly: 'Somente runtime global',
    windowsRuntime: 'Runtime ativo do Windows',
    effectiveRuntime: 'Runtime efetivo para o jogo',
    noRuntime: 'Nenhum runtime detectado',
    gameOverride: 'Override por jogo',
    systemDefault: 'Usar padrão do Windows',
    runtimes: 'Runtimes detectados',
    active: 'ativo no Windows',
    selected: 'selecionado para o jogo',
    disabled: 'desativado',
    missingManifest: 'manifest ausente',
    missingLibrary: 'biblioteca ausente',
    useForGame: 'Usar neste jogo',
    makeSystem: 'Tornar padrão do Windows',
    applying: 'Aplicando…',
    noRuntimes: 'Nenhum runtime OpenXR foi encontrado pelo registro do Windows ou pelos caminhos conhecidos.',
    gameHint: 'O override por jogo usa XR_RUNTIME_JSON somente no processo iniciado pelo Moddin. Ele não altera o runtime global do Windows.',
    systemHint: 'Alterar o runtime global grava HKLM\\SOFTWARE\\Khronos\\OpenXR\\1\\ActiveRuntime e exige UAC de administrador.',
    uacHint: 'O Windows pode abrir uma janela de confirmação de administrador.',
    sourceGame: 'override do jogo',
    sourceSystem: 'Windows',
    sourceNone: 'nenhum',
  },
  en: {
    button: 'OpenXR',
    title: 'OpenXR Manager',
    subtitle: 'Windows runtime and per-game override',
    close: 'Close',
    refresh: 'Refresh',
    selectGame: 'Game override',
    systemOnly: 'System runtime only',
    windowsRuntime: 'Windows active runtime',
    effectiveRuntime: 'Effective runtime for game',
    noRuntime: 'No runtime detected',
    gameOverride: 'Per-game override',
    systemDefault: 'Use Windows default',
    runtimes: 'Detected runtimes',
    active: 'active in Windows',
    selected: 'selected for game',
    disabled: 'disabled',
    missingManifest: 'manifest missing',
    missingLibrary: 'runtime library missing',
    useForGame: 'Use for this game',
    makeSystem: 'Make Windows default',
    applying: 'Applying…',
    noRuntimes: 'No OpenXR runtimes were found in the Windows registry or known runtime locations.',
    gameHint: 'The per-game override uses XR_RUNTIME_JSON only for games launched by Moddin. It does not change the Windows global runtime.',
    systemHint: 'Changing the global runtime writes HKLM\\SOFTWARE\\Khronos\\OpenXR\\1\\ActiveRuntime and requires administrator UAC.',
    uacHint: 'Windows may show an administrator confirmation prompt.',
    sourceGame: 'game override',
    sourceSystem: 'Windows',
    sourceNone: 'none',
  },
  es: {
    button: 'OpenXR',
    title: 'Gestor OpenXR',
    subtitle: 'Runtime de Windows y override por juego',
    close: 'Cerrar',
    refresh: 'Actualizar',
    selectGame: 'Override para juego',
    systemOnly: 'Solo runtime global',
    windowsRuntime: 'Runtime activo de Windows',
    effectiveRuntime: 'Runtime efectivo para el juego',
    noRuntime: 'Ningún runtime detectado',
    gameOverride: 'Override por juego',
    systemDefault: 'Usar predeterminado de Windows',
    runtimes: 'Runtimes detectados',
    active: 'activo en Windows',
    selected: 'seleccionado para el juego',
    disabled: 'desactivado',
    missingManifest: 'manifiesto ausente',
    missingLibrary: 'biblioteca ausente',
    useForGame: 'Usar en este juego',
    makeSystem: 'Hacer predeterminado de Windows',
    applying: 'Aplicando…',
    noRuntimes: 'No se encontraron runtimes OpenXR en el registro de Windows ni en ubicaciones conocidas.',
    gameHint: 'El override por juego usa XR_RUNTIME_JSON solo para juegos iniciados por Moddin. No cambia el runtime global de Windows.',
    systemHint: 'Cambiar el runtime global escribe HKLM\\SOFTWARE\\Khronos\\OpenXR\\1\\ActiveRuntime y requiere UAC de administrador.',
    uacHint: 'Windows puede mostrar una confirmación de administrador.',
    sourceGame: 'override del juego',
    sourceSystem: 'Windows',
    sourceNone: 'ninguno',
  },
} as const

const copy = computed(() => {
  const key = locale.value === 'pt-BR' || locale.value === 'es' ? locale.value : 'en'
  return messages[key]
})

const gameChoices = computed(() => installedGames.value.flatMap((game) => {
  const catalog = findCatalogGameBySteamAppId(game.appId)
  if (!catalog) return []
  return [{ gameId: catalog.id, label: game.name }]
}))

const selectedGameRuntime = computed(() => state.value?.gameOverride ?? null)

function isSelectedForGame(runtime: OpenXrRuntimeInfo) {
  return selectedGameRuntime.value?.toLowerCase() === runtime.manifestPath.toLowerCase()
}

function runtimeUsable(runtime: OpenXrRuntimeInfo) {
  return runtime.enabled && runtime.manifestExists && runtime.libraryExists
}

function effectiveSourceLabel() {
  if (state.value?.effectiveSource === 'game') return copy.value.sourceGame
  if (state.value?.effectiveSource === 'system') return copy.value.sourceSystem
  return copy.value.sourceNone
}

async function inspect() {
  loading.value = true
  error.value = null
  try {
    state.value = await invoke<OpenXrState>('inspect_openxr', {
      gameId: selectedGameId.value || null,
    })
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    loading.value = false
  }
}

async function loadGames() {
  try {
    installedGames.value = await invoke<InstalledGame[]>('detect_steam_games')
    const saved = window.localStorage.getItem('moddin-openxr-game') ?? ''
    if (saved && gameChoices.value.some((game) => game.gameId === saved)) {
      selectedGameId.value = saved
    } else if (!selectedGameId.value && gameChoices.value.length) {
      selectedGameId.value = gameChoices.value[0].gameId
    }
  } catch {
    installedGames.value = []
  }
}

async function openManager() {
  open.value = true
  await loadGames()
  await inspect()
}

async function setGameRuntime(manifestPath: string | null) {
  if (!selectedGameId.value) return
  busyAction.value = `game:${manifestPath ?? 'system'}`
  error.value = null
  try {
    state.value = await invoke<OpenXrState>('set_game_openxr_runtime', {
      gameId: selectedGameId.value,
      manifestPath,
    })
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    busyAction.value = null
  }
}

async function setSystemRuntime(manifestPath: string) {
  busyAction.value = `system:${manifestPath}`
  error.value = null
  try {
    state.value = await invoke<OpenXrState>('set_system_openxr_runtime', {
      manifestPath,
      gameId: selectedGameId.value || null,
    })
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    busyAction.value = null
  }
}

watch(selectedGameId, async (value) => {
  try {
    if (value) window.localStorage.setItem('moddin-openxr-game', value)
  } catch {
    // Storage is optional.
  }
  if (open.value) await inspect()
})
</script>

<template>
  <button class="openxr-fab" type="button" @click="openManager">
    <span class="openxr-dot"></span>
    {{ copy.button }}
  </button>

  <div v-if="open" class="openxr-backdrop" @click.self="open = false">
    <section class="openxr-panel" role="dialog" aria-modal="true" :aria-label="copy.title">
      <header class="openxr-header">
        <div>
          <small>OPENXR</small>
          <h2>{{ copy.title }}</h2>
          <p>{{ copy.subtitle }}</p>
        </div>
        <button class="openxr-icon-button" type="button" :aria-label="copy.close" @click="open = false">×</button>
      </header>

      <div class="openxr-toolbar">
        <label>
          <span>{{ copy.selectGame }}</span>
          <select v-model="selectedGameId">
            <option value="">{{ copy.systemOnly }}</option>
            <option v-for="game in gameChoices" :key="game.gameId" :value="game.gameId">{{ game.label }}</option>
          </select>
        </label>
        <button class="openxr-secondary" type="button" :disabled="loading || busyAction !== null" @click="inspect">
          {{ copy.refresh }}
        </button>
      </div>

      <div v-if="error" class="openxr-error">{{ error }}</div>
      <div v-if="loading && !state" class="openxr-empty">{{ copy.refresh }}…</div>

      <template v-if="state">
        <div class="openxr-summary">
          <div>
            <span>{{ copy.windowsRuntime }}</span>
            <strong>{{ state.activeRuntimeName ?? copy.noRuntime }}</strong>
            <code v-if="state.activeRuntime">{{ state.activeRuntime }}</code>
          </div>
          <div v-if="selectedGameId">
            <span>{{ copy.effectiveRuntime }} · {{ effectiveSourceLabel() }}</span>
            <strong>{{ state.effectiveRuntimeName ?? copy.noRuntime }}</strong>
            <code v-if="state.effectiveRuntime">{{ state.effectiveRuntime }}</code>
          </div>
        </div>

        <div v-if="selectedGameId" class="openxr-note">
          <strong>{{ copy.gameOverride }}</strong>
          <p>{{ copy.gameHint }}</p>
          <button
            class="openxr-secondary"
            type="button"
            :disabled="busyAction !== null || state.gameOverride === null"
            @click="setGameRuntime(null)"
          >
            {{ copy.systemDefault }}
          </button>
        </div>

        <div class="openxr-note">
          <strong>Windows</strong>
          <p>{{ copy.systemHint }}</p>
          <small>{{ copy.uacHint }}</small>
        </div>

        <div v-if="state.warnings.length" class="openxr-warnings">
          <p v-for="warning in state.warnings" :key="warning">{{ warning }}</p>
        </div>

        <div class="openxr-section-title">
          <h3>{{ copy.runtimes }}</h3>
          <span>{{ state.runtimes.length }}</span>
        </div>

        <div v-if="!state.runtimes.length" class="openxr-empty">{{ copy.noRuntimes }}</div>
        <div v-else class="openxr-runtime-list">
          <article v-for="runtime in state.runtimes" :key="runtime.manifestPath" class="openxr-runtime">
            <div class="openxr-runtime-heading">
              <div>
                <strong>{{ runtime.name }}</strong>
                <code>{{ runtime.manifestPath }}</code>
              </div>
              <div class="openxr-badges">
                <span v-if="runtime.active" class="good">{{ copy.active }}</span>
                <span v-if="isSelectedForGame(runtime)" class="selected">{{ copy.selected }}</span>
                <span v-if="!runtime.enabled" class="bad">{{ copy.disabled }}</span>
                <span v-if="!runtime.manifestExists" class="bad">{{ copy.missingManifest }}</span>
                <span v-else-if="!runtime.libraryExists" class="warn">{{ copy.missingLibrary }}</span>
              </div>
            </div>

            <div class="openxr-runtime-actions">
              <button
                v-if="selectedGameId"
                class="openxr-secondary"
                type="button"
                :disabled="!runtimeUsable(runtime) || busyAction !== null || isSelectedForGame(runtime)"
                @click="setGameRuntime(runtime.manifestPath)"
              >
                {{ busyAction === `game:${runtime.manifestPath}` ? copy.applying : copy.useForGame }}
              </button>
              <button
                class="openxr-primary"
                type="button"
                :disabled="!runtimeUsable(runtime) || busyAction !== null || runtime.active"
                @click="setSystemRuntime(runtime.manifestPath)"
              >
                {{ busyAction === `system:${runtime.manifestPath}` ? copy.applying : copy.makeSystem }}
              </button>
            </div>
          </article>
        </div>
      </template>
    </section>
  </div>
</template>

<style scoped>
.openxr-fab {
  position: fixed;
  right: 22px;
  bottom: 22px;
  z-index: 1500;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border: 1px solid rgba(148, 163, 184, 0.25);
  border-radius: 999px;
  color: #e5eefc;
  background: rgba(15, 23, 42, 0.94);
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.32);
  cursor: pointer;
  font: inherit;
  font-weight: 700;
}

.openxr-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #22c55e;
  box-shadow: 0 0 12px rgba(34, 197, 94, 0.8);
}

.openxr-backdrop {
  position: fixed;
  inset: 0;
  z-index: 2000;
  display: grid;
  place-items: center;
  padding: 28px;
  background: rgba(2, 6, 23, 0.76);
  backdrop-filter: blur(8px);
}

.openxr-panel {
  width: min(900px, 96vw);
  max-height: 90vh;
  overflow: auto;
  border: 1px solid rgba(148, 163, 184, 0.2);
  border-radius: 18px;
  background: #0f172a;
  color: #e5eefc;
  box-shadow: 0 30px 80px rgba(0, 0, 0, 0.48);
  padding: 22px;
}

.openxr-header,
.openxr-toolbar,
.openxr-runtime-heading,
.openxr-runtime-actions,
.openxr-section-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.openxr-header small,
.openxr-toolbar span,
.openxr-summary span {
  color: #94a3b8;
  font-size: 0.76rem;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.openxr-header h2 {
  margin: 3px 0 2px;
}

.openxr-header p,
.openxr-note p {
  margin: 0;
  color: #94a3b8;
}

.openxr-icon-button {
  border: 0;
  background: transparent;
  color: #cbd5e1;
  font-size: 1.8rem;
  cursor: pointer;
}

.openxr-toolbar {
  margin: 20px 0;
}

.openxr-toolbar label {
  display: grid;
  gap: 6px;
  flex: 1;
}

.openxr-toolbar select {
  width: 100%;
  padding: 9px 11px;
  border: 1px solid rgba(148, 163, 184, 0.22);
  border-radius: 9px;
  color: #e5eefc;
  background: #111c31;
}

.openxr-summary {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 12px;
}

.openxr-summary > div,
.openxr-note,
.openxr-runtime,
.openxr-error,
.openxr-warnings,
.openxr-empty {
  border: 1px solid rgba(148, 163, 184, 0.16);
  border-radius: 12px;
  background: rgba(30, 41, 59, 0.42);
  padding: 14px;
}

.openxr-summary > div {
  display: grid;
  gap: 6px;
}

.openxr-summary code,
.openxr-runtime code {
  overflow-wrap: anywhere;
  color: #93c5fd;
  font-size: 0.75rem;
}

.openxr-note {
  margin-top: 12px;
  display: grid;
  gap: 8px;
}

.openxr-note small {
  color: #fbbf24;
}

.openxr-warnings,
.openxr-error {
  margin-top: 12px;
  color: #fca5a5;
  border-color: rgba(239, 68, 68, 0.28);
}

.openxr-warnings p {
  margin: 4px 0;
}

.openxr-section-title {
  margin: 22px 0 10px;
}

.openxr-section-title h3 {
  margin: 0;
}

.openxr-section-title span {
  color: #94a3b8;
}

.openxr-runtime-list {
  display: grid;
  gap: 10px;
}

.openxr-runtime-heading {
  align-items: flex-start;
}

.openxr-runtime-heading > div:first-child {
  display: grid;
  gap: 5px;
  min-width: 0;
}

.openxr-badges {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 6px;
}

.openxr-badges span {
  padding: 3px 7px;
  border-radius: 999px;
  font-size: 0.7rem;
  font-weight: 700;
}

.openxr-badges .good { background: rgba(34, 197, 94, 0.15); color: #86efac; }
.openxr-badges .selected { background: rgba(59, 130, 246, 0.18); color: #93c5fd; }
.openxr-badges .warn { background: rgba(245, 158, 11, 0.15); color: #fcd34d; }
.openxr-badges .bad { background: rgba(239, 68, 68, 0.16); color: #fca5a5; }

.openxr-runtime-actions {
  justify-content: flex-end;
  margin-top: 12px;
}

.openxr-primary,
.openxr-secondary {
  border-radius: 9px;
  padding: 8px 11px;
  font: inherit;
  font-weight: 700;
  cursor: pointer;
}

.openxr-primary {
  border: 1px solid rgba(59, 130, 246, 0.6);
  background: #2563eb;
  color: white;
}

.openxr-secondary {
  border: 1px solid rgba(148, 163, 184, 0.24);
  background: rgba(30, 41, 59, 0.78);
  color: #dbeafe;
}

.openxr-primary:disabled,
.openxr-secondary:disabled {
  cursor: not-allowed;
  opacity: 0.45;
}

@media (max-width: 720px) {
  .openxr-backdrop { padding: 12px; }
  .openxr-panel { padding: 16px; }
  .openxr-toolbar,
  .openxr-runtime-heading,
  .openxr-runtime-actions { align-items: stretch; flex-direction: column; }
  .openxr-badges { justify-content: flex-start; }
  .openxr-primary,
  .openxr-secondary { width: 100%; }
}

/* Shared utility styling with the rest of Moddin. */
.openxr-fab {
  right: 22px;
  bottom: 22px;
  border-color: #2b3744;
  color: #e8edf3;
  background: rgba(16, 23, 32, 0.96);
}

.openxr-backdrop { background: rgba(3, 6, 10, 0.76); }
.openxr-panel { border-color: #2b3744; background: #101720; color: #e8edf3; }
.openxr-header small,
.openxr-toolbar span,
.openxr-summary span,
.openxr-header p,
.openxr-note p,
.openxr-section-title span { color: #8996a5; }
.openxr-toolbar select { border-color: #2b3744; color: #e8edf3; background: #0b1118; }
.openxr-summary > div,
.openxr-note,
.openxr-runtime,
.openxr-empty { border-color: #2a3642; background: rgba(20, 29, 39, 0.8); }
.openxr-summary code,
.openxr-runtime code { color: #c5bcff; }
.openxr-primary { border-color: #8268ff; background: #6951df; }
.openxr-secondary { border-color: #394756; color: #e8edf3; background: #18222d; }
.openxr-badges .selected { color: #c5bcff; background: rgba(130, 104, 255, 0.18); }

@media (max-width: 720px) {
  .openxr-fab { right: 16px; bottom: 16px; }
}
</style>
