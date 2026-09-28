import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAiAssistantTrigger } from './useAiAssistant'
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
  const { t } = useI18n()
  const aiModuleActions = useAiModuleActions()
  const trigger = useAiAssistantTrigger()
  const hasError = computed(() => Boolean(deps.currentError()))

  async function auditLike() {
    await aiModuleActions.openAiAudit({
      gameId: deps.selectedAppId(),
      gameName: deps.selectedGameName(),
    })
  }

  async function ask() { await auditLike() }

  async function recommend() {
    const gameId = deps.selectedAppId()
    if (!gameId) return
    const gameName = deps.selectedGameName() ?? t('aiAssistantGameBannerAnyGame')
    await trigger.openAiAssistantRecommendations({
      gameId,
      gameName,
      intent: t('aiAssistantGameBannerIntent', { game: gameName }),
    })
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
