import { computed, ref, watch } from 'vue'
import { useDialogLifecycle } from '../../composables/useDialogLifecycle'
import { findCatalogGameBySteamAppId } from '../../services/catalog'
import { readLocalValue, writeLocalValue } from '../../services/storage'
import type { InstalledGame } from '../../types/game'
import {
  detectOpenXrGames,
  inspectOpenXr,
  setGameOpenXrRuntime,
  setSystemOpenXrRuntime,
} from './service'
import type { OpenXrRuntimeInfo, OpenXrState } from './types'

const SELECTED_GAME_STORAGE_KEY = 'moddin-openxr-game'

export function useOpenXrManager() {
  const open = ref(false)
  const loading = ref(false)
  const busyAction = ref<string | null>(null)
  const error = ref<string | null>(null)
  const state = ref<OpenXrState | null>(null)
  const installedGames = ref<InstalledGame[]>([])
  const selectedGameId = ref('')
  const { dialogElement, openDialog, closeDialog } = useDialogLifecycle(open)

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

  async function inspect() {
    loading.value = true
    error.value = null
    try {
      state.value = await inspectOpenXr(selectedGameId.value || null)
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err)
    } finally {
      loading.value = false
    }
  }

  async function loadGames() {
    try {
      installedGames.value = await detectOpenXrGames()
      const saved = readLocalValue(SELECTED_GAME_STORAGE_KEY) ?? ''
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
    await openDialog()
    await loadGames()
    await inspect()
  }

  async function setGameRuntime(manifestPath: string | null) {
    if (!selectedGameId.value) return
    busyAction.value = `game:${manifestPath ?? 'system'}`
    error.value = null
    try {
      state.value = await setGameOpenXrRuntime(selectedGameId.value, manifestPath)
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
      state.value = await setSystemOpenXrRuntime(manifestPath, selectedGameId.value || null)
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err)
    } finally {
      busyAction.value = null
    }
  }

  watch(selectedGameId, async (value) => {
    if (value) writeLocalValue(SELECTED_GAME_STORAGE_KEY, value)
    if (open.value) await inspect()
  })

  return {
    open,
    dialogElement,
    loading,
    busyAction,
    error,
    state,
    selectedGameId,
    gameChoices,
    isSelectedForGame,
    runtimeUsable,
    inspect,
    openManager,
    closeManager: closeDialog,
    setGameRuntime,
    setSystemRuntime,
  }
}
