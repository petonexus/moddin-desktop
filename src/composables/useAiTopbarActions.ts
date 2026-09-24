import { computed } from 'vue'
import { useAiModuleActions } from './useAiModuleActions'

// Topbar IA split-button actions extracted from App.vue so it stays
// under its 115 KB byte budget. The split button has two halves: main
// (contextual default = audit), arrow (dropdown of modes). Each handler
// closes any open menu implicitly and routes through the existing AI
// composables so the singleton dialog opens with the right mode + game.

export function useAiTopbarActions(deps: {
  selectedAppId: () => string | null
  selectedGameName: () => string | null
  currentError: () => string | null
}) {
  const aiModuleActions = useAiModuleActions()
  const hasError = computed(() => Boolean(deps.currentError()))

  async function auditLike() {
    await aiModuleActions.openAiAudit({
      gameId: deps.selectedAppId(),
      gameName: deps.selectedGameName(),
    })
  }

  async function ask() { await auditLike() }

  async function recommend() {
    if (!deps.selectedAppId()) return
    await auditLike()
  }

  async function audit() { await auditLike() }

  function diagnose() {
    const message = deps.currentError()
    if (!message) return
    aiModuleActions.openDiagnoseWithAi({ message, gameId: deps.selectedAppId(), gameName: deps.selectedGameName(), capabilityId: null })
  }

  function contribute() {
    // ContributeDialog is mounted globally; signal it via a window event
    // so the architecture boundary stays clean.
    window.dispatchEvent(new CustomEvent('moddin:open-contribute'))
  }

  return { hasError, ask, recommend, audit, diagnose, contribute }
}
