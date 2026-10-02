import { computed, ref } from 'vue'
import { createSnapshot, deleteSnapshot, listSnapshots, rollbackSnapshot } from './service'
import type { Snapshot } from './types'

/** Which action produced `error`; the raw string alone cannot say. */
export type SnapshotAction = 'load' | 'create' | 'rollback' | 'delete'

/**
 * State for the History panel's snapshot section.
 *
 * The create-form fields live here rather than in the component because
 * the rule "a snapshot needs a name and a game" has to be enforced in
 * one place: the button reads `canCreate` instead of re-deriving the
 * same condition in the template, and `create` refuses a request that
 * would be rejected anyway.
 */
export function useSnapshots() {
  const snapshots = ref<Snapshot[]>([])
  const loading = ref(false)
  /** Id of the snapshot being worked on, so only that row spins. */
  const busyId = ref<string | null>(null)
  /** …and which of its two actions, so the right label spins. */
  const busyAction = ref<SnapshotAction | null>(null)
  const error = ref<string | null>(null)
  const errorContext = ref<SnapshotAction | null>(null)

  const name = ref('')
  const gameId = ref('')

  function fail(action: SnapshotAction, err: unknown) {
    error.value = err instanceof Error ? err.message : String(err)
    errorContext.value = action
  }

  async function refresh(options: { preserveError?: boolean } = {}) {
    if (loading.value || busyId.value !== null) return
    loading.value = true
    if (!options.preserveError) {
      error.value = null
      errorContext.value = 'load'
    }
    try {
      snapshots.value = await listSnapshots()
    } catch (err) {
      if (!options.preserveError || !error.value) fail('load', err)
    } finally {
      loading.value = false
    }
  }

  /**
   * Rust trims the name and rejects an empty one, so a field holding
   * spaces is empty here too. The game is a required explicit choice:
   * `create_snapshot` is per-game, and guessing which game the user
   * meant is how a restore point ends up on the wrong title.
   */
  const canCreate = computed(() => name.value.trim().length > 0 && gameId.value.length > 0)

  async function create() {
    if (!canCreate.value || loading.value || busyId.value !== null) return false
    busyId.value = 'new'
    busyAction.value = 'create'
    error.value = null
    errorContext.value = 'create'
    try {
      const created = await createSnapshot(name.value, gameId.value)
      // `list_snapshots` sorts newest first, so prepending is what a
      // refresh would have returned — without the round trip.
      snapshots.value = [created, ...snapshots.value.filter((item) => item.id !== created.id)]
      name.value = ''
      return true
    } catch (err) {
      fail('create', err)
      return false
    } finally {
      busyId.value = null
      busyAction.value = null
    }
  }

  /**
   * Rolls the captured transactions back and leaves the snapshot in
   * place, so the caller can report what changed and refresh the
   * transaction list. Returns whether it worked; the error itself is
   * already in `error` for the callout to explain.
   */
  async function rollback(id: string) {
    if (loading.value || busyId.value !== null) return false
    busyId.value = id
    busyAction.value = 'rollback'
    error.value = null
    errorContext.value = 'rollback'
    try {
      await rollbackSnapshot(id)
      return true
    } catch (err) {
      fail('rollback', err)
      return false
    } finally {
      busyId.value = null
      busyAction.value = null
    }
  }

  async function remove(id: string) {
    if (loading.value || busyId.value !== null) return false
    busyId.value = id
    busyAction.value = 'delete'
    error.value = null
    errorContext.value = 'delete'
    try {
      await deleteSnapshot(id)
      snapshots.value = snapshots.value.filter((item) => item.id !== id)
      return true
    } catch (err) {
      fail('delete', err)
      return false
    } finally {
      busyId.value = null
      busyAction.value = null
    }
  }

  return {
    snapshots,
    loading,
    busyId,
    busyAction,
    error,
    errorContext,
    name,
    gameId,
    canCreate,
    refresh,
    create,
    rollback,
    remove,
  }
}
