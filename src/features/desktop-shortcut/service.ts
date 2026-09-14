import { invokeDebug as invoke } from '../../debug'
import type { GameCatalogEntry, InstalledGame, ToolModuleDefinition } from '../../types/game'
import type { DesktopShortcutPreview, DesktopShortcutRequest, DesktopShortcutResult } from './types'

export const DESKTOP_SHORTCUT_MODULE_ID = 'desktop-shortcut'

export function buildDesktopShortcutRequest(
  module: ToolModuleDefinition,
  game: { installed: InstalledGame; catalog?: GameCatalogEntry } | null,
): DesktopShortcutRequest | null {
  if (module.id !== DESKTOP_SHORTCUT_MODULE_ID || !game?.catalog) return null

  return {
    gameId: game.catalog.id,
    gameName: game.catalog.name,
    installDir: game.installed.installDir,
    executable: game.catalog.executable,
  }
}

export function previewDesktopShortcut(request: DesktopShortcutRequest) {
  return invoke<DesktopShortcutPreview>('preview_desktop_shortcut', { request })
}

export function createDesktopShortcut(request: DesktopShortcutRequest) {
  return invoke<DesktopShortcutResult>('create_desktop_shortcut', { request })
}
