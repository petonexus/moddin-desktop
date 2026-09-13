import { computed, ref } from 'vue'
import { useDialogLifecycle } from '../../composables/useDialogLifecycle'
import { clearActionLogs, listActionLogs } from './service'
import type { ActionLogEntry, ActionLogLevel } from './types'

export function useActivityLogPanel() {
  const open = ref(false)
  const loading = ref(false)
  const clearing = ref(false)
  const error = ref<string | null>(null)
  const logs = ref<ActionLogEntry[]>([])
  const search = ref('')
  const level = ref<'all' | ActionLogLevel>('all')
  const expanded = ref(new Set<string>())
  const { openDialog, closeDialog } = useDialogLifecycle(open)

  const filteredLogs = computed(() => {
    const term = search.value.trim().toLowerCase()
    return logs.value.filter((entry) => {
      if (level.value !== 'all' && entry.level !== level.value) return false
      if (!term) return true
      return [entry.action, entry.gameId, entry.message, entry.transactionId]
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
    openDialog()
    await refresh()
  }

  async function clearLogs(confirmMessage: string) {
    if (!window.confirm(confirmMessage)) return
    clearing.value = true
    error.value = null
    try {
      await clearActionLogs()
      logs.value = []
      expanded.value = new Set()
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err)
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
