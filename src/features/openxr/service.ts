import { invokeDebug as invoke } from '../../debug'
import type { InstalledGame } from '../../types/game'
import type { OpenXrState } from './types'

export function detectOpenXrGames() {
  return invoke<InstalledGame[]>('detect_installed_games')
}

export function inspectOpenXr(gameId: string | null) {
  return invoke<OpenXrState>('inspect_openxr', { gameId })
}

export function setGameOpenXrRuntime(gameId: string, manifestPath: string | null) {
  return invoke<OpenXrState>('set_game_openxr_runtime', {
    gameId,
    manifestPath,
  })
}

export function setSystemOpenXrRuntime(manifestPath: string, gameId: string | null) {
  return invoke<OpenXrState>('set_system_openxr_runtime', {
    manifestPath,
    gameId,
  })
}
