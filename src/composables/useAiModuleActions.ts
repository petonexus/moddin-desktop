import { useI18n } from 'vue-i18n'
import { useAiAssistantTrigger } from './useAiAssistant'
import type { AuthorPromptContext } from '../types/ai-assistant'

interface ModuleLike {
  id: string
  name?: string
}

/**
 * Extracts the AI-trigger actions used inside the game detail view
 * (module-cards, error banner, banner CTA). Keeps `App.vue` from
 * growing past its 115 KB budget by hoisting all the AI glue out of
 * the main script.
 */
export function useAiModuleActions() {
  const { t } = useI18n()
  const trigger = useAiAssistantTrigger()

  function openImproveWithAi(opts: {
    moduleId: string
    gameId: string | null
    gameName: string | null
  }) {
    const ctx: AuthorPromptContext = {
      mode: 'improve',
      gameId: opts.gameId,
      gameName: opts.gameName,
      capabilityId: opts.moduleId,
      intent: '',
    }
    trigger.openFor(ctx)
  }

  function openDiagnoseWithAi(opts: {
    message: string
    gameId: string | null
    gameName: string | null
    capabilityId: string | null
  }) {
    const ctx: AuthorPromptContext = {
      mode: 'diagnose',
      gameId: opts.gameId,
      gameName: opts.gameName,
      capabilityId: opts.capabilityId,
      intent: '',
      errorMessage: opts.message,
      errorSource: 'module',
    }
    trigger.openFor(ctx)
  }

  async function openWhyThisAi(opts: {
    module: ModuleLike
    gameId: string | null
    gameName: string | null
  }) {
    const moduleLabel = opts.module.name ?? opts.module.id
    const gameLabel = opts.gameName ?? ''
    await trigger.openAiAssistantRecommendations({
      gameId: opts.gameId,
      gameName: opts.gameName,
      intent: t('aiAssistantWhyThisIntent', {
        module: moduleLabel,
        game: gameLabel,
      }),
      capabilityId: opts.module.id,
    })
  }

  async function openAiAudit(opts: {
    gameId: string | null
    gameName: string | null
    intent?: string
  }) {
    const gameLabel = opts.gameName ?? opts.gameId ?? ''
    await trigger.openAiAssistantRecommendations({
      gameId: opts.gameId,
      gameName: opts.gameName,
      intent: opts.intent ?? t('aiAssistantAuditIntent', { game: gameLabel }),
    })
  }

  return {
    openImproveWithAi,
    openDiagnoseWithAi,
    openWhyThisAi,
    openAiAudit,
  }
}
