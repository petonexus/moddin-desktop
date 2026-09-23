import { computed, ref } from 'vue'
import {
  abortCollection,
  getCollection,
  installCollection,
  listCollections,
  resumeCollection,
} from '../features/collection/service'
import type {
  CollectionAbortResult,
  CollectionInstallResult,
  CollectionResumeResult,
  CollectionSpec,
  CollectionSummary,
  CollectionSessionView,
  ResumeDecision,
} from '../types/collection'

interface InstallContext {
  gameId: string
  gameName: string
  installDir: string
  executableDir: string
}

/**
 * Composable that drives the "install a collection" workflow.
 *
 * State machine (mirrors the backend `CollectionSession`):
 *   idle → starting → running → (paused → continuing → running) → completed | aborted | error
 *
 * The backend is the source of truth — this composable just reflects
 * the session view it returns. When the backend signals `paused`, the
 * UI must ask the user and call `resume` with their decision.
 */
export function useCollectionInstall() {
  const collections = ref<CollectionSummary[]>([])
  const collectionsLoading = ref(false)
  const collectionsError = ref<string | null>(null)

  const session = ref<CollectionSessionView | null>(null)
  const phase = ref<
    | 'idle'
    | 'starting'
    | 'running'
    | 'continuing'
    | 'paused'
    | 'completed'
    | 'aborted'
    | 'error'
  >('idle')
  const lastError = ref<string | null>(null)
  const rollbackReport = ref<CollectionAbortResult | null>(null)

  async function refreshCollections() {
    collectionsLoading.value = true
    collectionsError.value = null
    try {
      collections.value = await listCollections()
    } catch (err) {
      collectionsError.value = err instanceof Error ? err.message : String(err)
    } finally {
      collectionsLoading.value = false
    }
  }

  function reset() {
    session.value = null
    phase.value = 'idle'
    lastError.value = null
    rollbackReport.value = null
  }

  function applySession(result: CollectionInstallResult | CollectionResumeResult) {
    session.value = result.session
    if (result.paused) {
      phase.value = 'paused'
      lastError.value = result.error ?? result.session.lastError
      return
    }
    if (!result.ok) {
      phase.value = 'error'
      lastError.value = result.error ?? result.session.lastError ?? 'Unknown error'
      return
    }
    if (result.session.status === 'completed') {
      phase.value = 'completed'
      return
    }
    if (result.session.status === 'closed') {
      phase.value = result.session.lastError ? 'error' : 'aborted'
      lastError.value = result.session.lastError
      return
    }
    phase.value = 'running'
  }

  async function loadDetail(id: string): Promise<CollectionSpec> {
    return getCollection(id)
  }

  async function start(collectionId: string, ctx: InstallContext) {
    reset()
    phase.value = 'starting'
    try {
      const result = await installCollection({
        collectionId,
        gameId: ctx.gameId,
        gameName: ctx.gameName,
        installDir: ctx.installDir,
        executableDir: ctx.executableDir,
      })
      applySession(result)
    } catch (err) {
      phase.value = 'error'
      lastError.value = err instanceof Error ? err.message : String(err)
    }
  }

  async function decide(decision: ResumeDecision) {
    if (!session.value) return
    phase.value = decision === 'abort' ? 'aborted' : 'continuing'
    try {
      const result = await resumeCollection(session.value.id, decision)
      applySession(result)
    } catch (err) {
      phase.value = 'error'
      lastError.value = err instanceof Error ? err.message : String(err)
    }
  }

  async function abort() {
    if (!session.value) return
    phase.value = 'aborted'
    try {
      const result = await abortCollection(session.value.id)
      rollbackReport.value = result
      session.value = result.session
      lastError.value = result.errors.length > 0 ? result.errors.join('\n') : null
    } catch (err) {
      phase.value = 'error'
      lastError.value = err instanceof Error ? err.message : String(err)
    }
  }

  const isPaused = computed(() => phase.value === 'paused')
  const isFinished = computed(
    () => phase.value === 'completed' || phase.value === 'aborted' || phase.value === 'error',
  )
  const progress = computed(() => {
    if (!session.value || session.value.totalSteps === 0) return 0
    const done = session.value.steps.filter(
      (s) => s.status === 'completed' || s.status === 'failed' || s.status === 'skipped',
    ).length
    return Math.round((done / session.value.totalSteps) * 100)
  })

  return {
    collections,
    collectionsLoading,
    collectionsError,
    session,
    phase,
    lastError,
    rollbackReport,
    progress,
    isPaused,
    isFinished,
    refreshCollections,
    loadDetail,
    start,
    decide,
    abort,
    reset,
  }
}
