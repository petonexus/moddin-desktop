import { invokeDebug as invoke } from '../../debug'
import { openXrWarningText } from '../../i18n/backendIds'
import type { InstalledGame } from '../../types/game'
import type { OpenXrState, OpenXrStateWire } from './types'

export function detectOpenXrGames() {
  return invoke<InstalledGame[]>('detect_installed_games')
}

export function inspectOpenXr(gameId: string | null) {
  return invoke<OpenXrStateWire>('inspect_openxr', { gameId }).then(withWarningKeys)
}

export function setGameOpenXrRuntime(gameId: string, manifestPath: string | null) {
  return invoke<OpenXrStateWire>('set_game_openxr_runtime', {
    gameId,
    manifestPath,
  }).then(withWarningKeys)
}

export function setSystemOpenXrRuntime(manifestPath: string, gameId: string | null) {
  return invoke<OpenXrStateWire>('set_system_openxr_runtime', {
    manifestPath,
    gameId,
  }).then(withWarningKeys)
}

/**
 * UX-21: every command that returns the whole state runs the warnings
 * through the boundary, so none of the three can reintroduce the raw
 * sentence. A warning the table does not declare keeps the backend's own
 * text — see the fallback note in `src/i18n/backendIds.ts`.
 */
function withWarningKeys(state: OpenXrStateWire): OpenXrState {
  return { ...state, warnings: (state.warnings ?? []).map(openXrWarningText) }
}
