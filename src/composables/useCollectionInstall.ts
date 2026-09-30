import { computed, ref } from 'vue'
import { listCollections } from '../features/collection/service'
import { installCommunityCapability } from '../features/community/service'
import { resolveCapabilityConfig } from '../features/capability-modules/config'
import {
  getCapabilitySpec,
  installCapability,
  listCapabilities,
  resolveInstallTarget,
  uninstallCapability,
  type CapabilityInstallTarget,
} from '../features/capability-modules/service'
import type { CapabilityConfigValue } from '../features/capability-modules/types'
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
 * path — one command per member, each recorded by the transaction
 * system. Which command is the point: a collection's members are
 * built-in recipes, so they go through `capability_install`, the same
 * command the game page's cards use. Sending them to
 * `community_capability_install` (which is what this did) was both the
 * wrong command and an empty config, so nothing could ever install.
 *
 * The branch this was ported from had a parallel `collection_install`
 * Tauri command with its own session, pause/resume and abort protocol;
 * that Rust module is not in this build and reintroducing it would be
 * the second install path this codebase forbids. What survives is the
 * product behaviour: an ordered step list, a stop-and-ask when a step
 * fails, and a revert of everything the run managed to install, all
 * recorded by the transaction system as usual.
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
  const errorCode = ref<'noGame' | 'noPreset' | 'needsConfig' | null>(null)

  /**
   * Members that could not be installed because a required config field
   * has no value — no catalogue entry for this game, no default in the
   * recipe. A collection run has no form to ask on, so it says which
   * member needs what and stops, rather than sending a blank and
   * failing inside the first step it had already begun.
   */
  const missingConfig = ref<Array<{ capabilityId: string; fields: string[] }>>([])

  /**
   * The resolved install target for the current run. Held here rather
   * than in the session because it is a local filesystem detail, and the
   * session is what the dialog renders.
   */
  const installTarget = ref<CapabilityInstallTarget | null>(null)

  /** Config per member id, resolved once when the run is planned. */
  const memberConfig = new Map<string, Record<string, CapabilityConfigValue>>()

  /**
   * Ids the capability registry knows — the built-in recipes and the
   * user's local ones. A member outside this list is a community
   * capability, which the loader refuses at load time (it is fetched and
   * signature-checked at install time, which a preview cannot promise).
   * The routing below is still written for both, because the refusal is
   * the backend's verdict and this run should not fail differently
   * because of it.
   */
  const registeredIds = new Set<string>()

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
    missingConfig.value = []
    rollbackReport.value = null
    installTarget.value = null
    memberConfig.clear()
  }

  /**
   * Install one member, on the command its origin asks for.
   *
   * A registered capability is a built-in recipe: `capability_install`,
   * with the per-game config the catalogue holds. Anything else is a
   * community recipe, and it goes through `community_capability_install`
   * with `acceptUnsigned: false` — fail closed, exactly like the AI flow:
   * a collection may not wave through an unsigned community recipe the
   * user has not acknowledged on the Community panel.
   */
  async function installMember(
    step: { capabilityId: string },
    target: CapabilityInstallTarget,
  ): Promise<{ transactionId: string | null }> {
    const config = { values: memberConfig.get(step.capabilityId) ?? {} }
    if (registeredIds.has(step.capabilityId)) {
      const result = await installCapability({
        capabilityId: step.capabilityId,
        gameId: target.gameId,
        gameName: target.gameName,
        installDir: target.installDir,
        executableDir: target.executableDir,
        config,
      })
      return { transactionId: result.transaction?.id ?? null }
    }
    const result = await installCommunityCapability({
      capabilityId: step.capabilityId,
      gameId: target.gameId,
      gameName: target.gameName,
      installDir: target.installDir,
      executableDir: target.executableDir,
      config,
      acceptUnsigned: false,
    })
    return { transactionId: result.transaction?.id ?? null }
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
        const { transactionId } = await installMember(step, target)
        step.transactionId = transactionId
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

  /**
   * Resolve every member's config before the first install: the recipe
   * says which fields it needs, the catalogue says what this game's build
   * of it needs, and a field neither answers for stops the run here
   * rather than inside step one.
   *
   * A member the registry does not know is a community recipe, and its
   * YAML is fetched and parsed by the backend at install time — this side
   * has no schema for it, and the signed catalogue is not a place to
   * guess one. Those members are planned as empty and left to the
   * community command, which is the only layer that has read the recipe
   * by then; the loader has already refused a collection that names one.
   */
  async function planMemberConfig(gameId: string, members: Array<{ id: string }>) {
    missingConfig.value = []
    for (const member of members) {
      if (!registeredIds.has(member.id)) {
        memberConfig.set(member.id, {})
        continue
      }
      try {
        const spec = await getCapabilitySpec(member.id)
        const resolved = resolveCapabilityConfig({
          gameId,
          capabilityId: member.id,
          schema: spec.configSchema ?? [],
        })
        memberConfig.set(member.id, resolved.values)
        if (resolved.missingRequired.length > 0) {
          missingConfig.value.push({ capabilityId: member.id, fields: resolved.missingRequired })
        }
      } catch (err) {
        // A registered recipe this build cannot read cannot be checked for
        // required fields, and an unchecked install is the thing this
        // refuses to do. Named, not swallowed.
        memberConfig.set(member.id, {})
        missingConfig.value.push({ capabilityId: member.id, fields: [messageOf(err)] })
      }
    }
    return missingConfig.value.length === 0
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

    // The registry decides which command a member installs through, so
    // ask it once per run rather than guessing from the id. A failure
    // here is not fatal: with an empty registry every member is treated
    // as a community recipe, which is the conservative direction — that
    // command refuses anything unsigned.
    try {
      const summaries = await listCapabilities()
      registeredIds.clear()
      for (const item of summaries) registeredIds.add(item.id)
    } catch {
      registeredIds.clear()
    }

    if (!(await planMemberConfig(target.gameId, preset.capabilities))) {
      errorCode.value = 'needsConfig'
      errorContext.value = 'install'
      phase.value = 'error'
      return
    }

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
    missingConfig,
    rollbackReport,
    progress,
    refreshCollections,
    start,
    decide,
    abort,
    reset,
  }
}
