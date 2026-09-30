import { computed, ref } from 'vue'
import { listCollections } from '../features/collection/service'
import { installCommunityCapability } from '../features/community/service'
import {
  resolveInstallTarget,
  uninstallCapability,
  type CapabilityInstallTarget,
} from '../features/capability-modules/service'
import type {
  CollectionPhase,
  CollectionRollbackReport,
  CollectionSession,
  CollectionSummary,
} from '../types/collection'

/**
 * Composable that drives the "install a collection" workflow.
 *
 * The run is a frontend loop over the **existing** capability install
 * path (`community_capability_install`) — the same command the Community
 * panel and the AI recommender use. The branch this was ported from had
 * a parallel `collection_install` Tauri command with its own session,
 * pause/resume and abort protocol; that Rust module is not in this build
 * and reintroducing it would be the second install path this codebase
 * forbids. What survives is the product behaviour: an ordered step list,
 * a stop-and-ask when a step fails, and a revert of everything the run
 * managed to install, all recorded by the transaction system as usual.
 *
 * Phase:
 *   idle -> installing -> completed
 *                   \-> paused -> installing   (the user chose to continue)
 *                           \-> reverting -> reverted | error
 *                   \-> error                  (nothing could be resolved)
 */
export function useCollectionInstall() {
  const collections = ref<CollectionSummary[]>([])
  const collectionsLoading = ref(false)
  const collectionsError = ref<string | null>(null)
  const collectionsErrorContext = ref<string | null>(null)

  const session = ref<CollectionSession | null>(null)
  const phase = ref<CollectionPhase>('idle')
  const error = ref<string | null>(null)
  const errorContext = ref<string | null>(null)
  const rollbackReport = ref<CollectionRollbackReport | null>(null)

  /**
   * Failures the dialog can phrase better than the raw string can.
   * The dialog turns these into localized copy and passes that copy to
   * `useFriendlyError` as `verbatim`, so they are not re-titled as
   * "something went wrong". Everything else is a backend message.
   */
  const errorCode = ref<'noGame' | 'noPreset' | null>(null)

  /**
   * The resolved install target for the current run. Held here rather
   * than in the session because it is a local filesystem detail, and the
   * session is what the dialog renders.
   */
  const installTarget = ref<CapabilityInstallTarget | null>(null)

  function messageOf(err: unknown): string {
    return err instanceof Error ? err.message : String(err)
  }

  async function refreshCollections() {
    collectionsLoading.value = true
    collectionsError.value = null
    collectionsErrorContext.value = 'load'
    try {
      collections.value = await listCollections()
    } catch (err) {
      collectionsError.value = messageOf(err)
    } finally {
      collectionsLoading.value = false
    }
  }

  function reset() {
    session.value = null
    phase.value = 'idle'
    error.value = null
    errorContext.value = null
    errorCode.value = null
    rollbackReport.value = null
    installTarget.value = null
  }

  /** Run every step from `fromIndex` onwards against a resolved target. */
  async function runFrom(fromIndex: number, target: CapabilityInstallTarget) {
    const current = session.value
    if (!current) return
    phase.value = 'installing'
    error.value = null
    for (let index = fromIndex; index < current.steps.length; index += 1) {
      const step = current.steps[index]
      current.nextIndex = index
      step.status = 'running'
      try {
        const result = await installCommunityCapability({
          capabilityId: step.capabilityId,
          gameId: target.gameId,
          gameName: target.gameName,
          installDir: target.installDir,
          executableDir: target.executableDir,
          config: {},
          // Fail closed, exactly like the AI flow: a collection may not
          // wave through an unsigned community recipe the user has not
          // acknowledged on the Community panel.
          acceptUnsigned: false,
        })
        step.transactionId = result.transaction?.id ?? null
        step.status = 'completed'
      } catch (err) {
        step.status = 'failed'
        step.error = messageOf(err)
        current.lastError = step.error
        current.failedCapability = step.capabilityId
        // The raw string goes to the callout, which turns it into a
        // summary plus something the user can act on. It is never the
        // only thing on screen.
        error.value = step.error
        errorContext.value = 'install'
        phase.value = 'paused'
        return
      }
    }
    current.nextIndex = current.steps.length
    phase.value = 'completed'
  }

  async function start(summary: CollectionSummary, gameId: string | null) {
    reset()
    const preset = summary.preset
    if (!preset || preset.capabilities.length === 0) {
      errorCode.value = 'noPreset'
      phase.value = 'error'
      return
    }

    session.value = {
      collectionId: summary.id,
      displayName: summary.displayName || summary.id,
      gameId: gameId ?? '',
      gameName: '',
      totalSteps: preset.capabilities.length,
      nextIndex: 0,
      steps: preset.capabilities.map((capability) => ({
        capabilityId: capability.id,
        displayName: capability.displayName || capability.id,
        status: 'pending' as const,
        transactionId: null,
        error: null,
      })),
      lastError: null,
      failedCapability: null,
    }

    let target: CapabilityInstallTarget | null
    try {
      target = await resolveInstallTarget(gameId)
    } catch (err) {
      error.value = messageOf(err)
      errorContext.value = 'install'
      phase.value = 'error'
      return
    }
    if (!target) {
      errorCode.value = 'noGame'
      phase.value = 'error'
      return
    }
    installTarget.value = target
    session.value.gameId = target.gameId
    session.value.gameName = target.gameName
    await runFrom(0, target)
  }

  /** The pause prompt's two answers: keep going, or undo what landed. */
  async function decide(decision: 'continue' | 'abort') {
    const current = session.value
    const target = installTarget.value
    if (!current || !target) return
    if (decision === 'abort') {
      await abort()
      return
    }
    // `nextIndex` still points at the step that failed, so resume after it.
    await runFrom(current.nextIndex + 1, target)
  }

  /**
   * Undo everything this run installed, newest first, through the same
   * capability uninstall the mod cards use — so each removal is recorded
   * as its own transaction and the whole revert is undoable too.
   */
  async function abort() {
    const current = session.value
    const target = installTarget.value
    if (!current || !target) return
    phase.value = 'reverting'
    error.value = null
    errorContext.value = 'revert'
    const report: CollectionRollbackReport = { reverted: [], errors: [] }
    for (let index = current.steps.length - 1; index >= 0; index -= 1) {
      const step = current.steps[index]
      if (step.status !== 'completed') continue
      try {
        await uninstallCapability({
          capabilityId: step.capabilityId,
          gameId: target.gameId,
          installDir: target.installDir,
        })
        step.status = 'reverted'
        report.reverted.push(step.capabilityId)
      } catch (err) {
        report.errors.push(`${step.capabilityId}: ${messageOf(err)}`)
      }
    }
    rollbackReport.value = report
    if (report.errors.length > 0) {
      error.value = report.errors.join('\n')
      phase.value = 'error'
      return
    }
    phase.value = 'reverted'
  }

  const progress = computed(() => {
    const current = session.value
    if (!current || current.totalSteps === 0) return 0
    const done = current.steps.filter(
      (step) => step.status === 'completed' || step.status === 'failed' || step.status === 'reverted',
    ).length
    return Math.round((done / current.totalSteps) * 100)
  })

  return {
    collections,
    collectionsLoading,
    collectionsError,
    collectionsErrorContext,
    session,
    phase,
    error,
    errorContext,
    errorCode,
    rollbackReport,
    progress,
    refreshCollections,
    start,
    decide,
    abort,
    reset,
  }
}
