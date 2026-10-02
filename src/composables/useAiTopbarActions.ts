import { useAiAssistantTrigger } from './useAiAssistant'
import { useAiModuleActions } from './useAiModuleActions'

/**
 * Topbar AI wiring, extracted from App.vue so it stays under its byte
 * budget.
 *
 * The menu used to offer four verbs for the same dialog. There is one
 * verb now: it opens the assistant with whatever game is selected, and
 * the dialog's mode picker decides what happens next. The one exception
 * is `diagnose`, which the library's error callout uses — there the mode
 * is not a choice, the error *is* the request.
 */
export function useAiTopbarActions(deps: {
  selectedGameId: () => string | null
  selectedGameName: () => string | null
  currentError: () => string | null
}) {
  const aiModuleActions = useAiModuleActions()
  const trigger = useAiAssistantTrigger()

  async function ask() {
    await trigger.openFor({
      mode: 'author',
      gameId: deps.selectedGameId(),
      gameName: deps.selectedGameName(),
      intent: '',
    })
  }

  function diagnose() {
    const message = deps.currentError()
    if (!message) return
    aiModuleActions.openDiagnoseWithAi({
      message,
      gameId: deps.selectedGameId(),
      gameName: deps.selectedGameName(),
      capabilityId: null,
    })
  }

  return { ask, diagnose }
}
