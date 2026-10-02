import { computed, ref } from 'vue'
import { clearActionLogs, listActionLogs } from './service'
import type { ActionLogEntry, ActionLogLevel } from './types'

export function useActivityLogPanel(options: { actionLabel?: (entry: ActionLogEntry) => string } = {}) {
  const open = ref(false)
  const loading = ref(false)
  const clearing = ref(false)
  const error = ref<string | null>(null)
  const logs = ref<ActionLogEntry[]>([])
  const search = ref('')
  const level = ref<'all' | ActionLogLevel>('all')
  const expanded = ref(new Set<string>())

  // Focus, Escape and focus restore are BaseDialog's job; this only has to
  // say whether the dialog is on screen.
  function openDialog() {
    open.value = true
  }

  function closeDialog() {
    open.value = false
  }

  const filteredLogs = computed(() => {
    const term = search.value.trim().toLowerCase()
    return logs.value.filter((entry) => {
      if (level.value !== 'all' && entry.level !== level.value) return false
      if (!term) return true
      // The command name is searched as well as the row's other fields, so
      // someone who remembers `install_optiscaler` finds the row even
      // though the row now says "Instalar o OptiScaler".
      return [entry.action.text, options.actionLabel?.(entry), entry.gameId, entry.message, entry.transactionId]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(term))
    })
  })

  function toggleDetails(id: string) {
    const next = new Set(expanded.value)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    expanded.value = next
  }

  async function refresh() {
    if (loading.value || clearing.value) return
    loading.value = true
    error.value = null
    try {
      logs.value = await listActionLogs()
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err)
    } finally {
      loading.value = false
    }
  }

  async function openPanel() {
    await openDialog()
    await refresh()
  }

  async function clearLogs() {
    if (loading.value || clearing.value) return false
    clearing.value = true
    error.value = null
    try {
      await clearActionLogs()
      logs.value = []
      expanded.value = new Set()
      return true
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err)
      return false
    } finally {
      clearing.value = false
    }
  }

  return {
    open,
    loading,
    clearing,
    error,
    logs,
    search,
    level,
    expanded,
    filteredLogs,
    toggleDetails,
    refresh,
    openPanel,
    closePanel: closeDialog,
    clearLogs,
  }
}
