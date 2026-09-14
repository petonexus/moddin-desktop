import { ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { GameCatalogEntry, InstalledGame, ToolModuleDefinition } from '../../types/game'
import { buildDesktopShortcutRequest, createDesktopShortcut, previewDesktopShortcut } from './service'
import type { DesktopShortcutPreview, DesktopShortcutRequest } from './types'

type SelectedGame = { installed: InstalledGame; catalog?: GameCatalogEntry } | null

interface DesktopShortcutHost {
  busy: Ref<boolean>
  actionError: Ref<string | null>
  success: Ref<string | null>
  onCreated: (module: ToolModuleDefinition) => Promise<void>
}

export function useDesktopShortcut(host: DesktopShortcutHost) {
  const { t } = useI18n()
  const dialog = ref<{
    module: ToolModuleDefinition
    request: DesktopShortcutRequest
    preview: DesktopShortcutPreview
  } | null>(null)

  function close() {
    dialog.value = null
  }

  function reportError(error: unknown) {
    host.actionError.value = error instanceof Error ? error.message : String(error)
  }

  // Returns true when the module was handled, so configureModule can stop there.
  async function open(module: ToolModuleDefinition, game: SelectedGame) {
    const request = buildDesktopShortcutRequest(module, game)
    if (!request) return false

    host.busy.value = true
    try {
      dialog.value = { module, request, preview: await previewDesktopShortcut(request) }
    } catch (error) {
      reportError(error)
    } finally {
      host.busy.value = false
    }
    return true
  }

  async function confirm() {
    if (!dialog.value) return

    host.actionError.value = null
    host.success.value = null
    host.busy.value = true
    try {
      const { module, request } = dialog.value
      const result = await createDesktopShortcut(request)
      dialog.value = null
      host.success.value = t('desktopShortcutCreated', { path: result.shortcutPath })
      await host.onCreated(module)
    } catch (error) {
      reportError(error)
    } finally {
      host.busy.value = false
    }
  }

  async function preview(module: ToolModuleDefinition, game: SelectedGame) {
    const request = buildDesktopShortcutRequest(module, game)
    if (!request) throw new Error(t('moduleNoAction', { module: module.id }))
    return previewDesktopShortcut(request)
  }

  return { dialog, open, confirm, close, preview }
}
