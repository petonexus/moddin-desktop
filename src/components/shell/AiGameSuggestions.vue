<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAiAssistantTrigger } from '../../composables/useAiAssistant'
import { useAiModuleActions } from '../../composables/useAiModuleActions'

const props = defineProps<{
  gameId: string | null
  gameName: string | null
}>()

const { t } = useI18n()
const { openAiAssistantRecommendations } = useAiAssistantTrigger()
const { openAiAudit } = useAiModuleActions()

const dismissed = ref(false)

const gameLabel = computed(() => props.gameName ?? props.gameId ?? t('aiAssistantGameBannerAnyGame'))
const isVisible = computed(() => Boolean(props.gameId) && !dismissed.value)

async function ask() {
  await openAiAssistantRecommendations({
    gameId: props.gameId,
    gameName: props.gameName,
    intent: t('aiAssistantGameBannerIntent', { game: gameLabel.value }),
  })
}

async function audit() {
  await openAiAudit({
    gameId: props.gameId,
    gameName: props.gameName,
    intent: t('aiAssistantAuditIntent', { game: gameLabel.value }),
  })
}

function dismiss() {
  dismissed.value = true
}
</script>

<template>
  <aside v-if="isVisible" class="ai-game-banner">
    <div class="ai-game-banner-icon" aria-hidden="true">✨</div>
    <div class="ai-game-banner-body">
      <strong>{{ t('aiAssistantGameBannerHeading') }}</strong>
      <p>{{ t('aiAssistantGameBannerBody', { game: gameLabel }) }}</p>
    </div>
    <div class="ai-game-banner-actions">
      <button class="ai-game-banner-secondary" type="button" @click="audit">
        {{ t('aiAssistantAuditButton') }}
      </button>
      <button class="ai-game-banner-primary" type="button" @click="ask">
        {{ t('aiAssistantGameBannerAsk') }}
      </button>
      <button
        class="ai-game-banner-dismiss"
        type="button"
        :aria-label="t('aiAssistantGameBannerDismiss')"
        @click="dismiss"
      >
        ×
      </button>
    </div>
  </aside>
</template>

<style scoped>
.ai-game-banner {
  display: grid;
  grid-template-columns: auto 1fr auto;
  align-items: center;
  gap: 0.85rem;
  background: var(--moddin-surface-2, rgba(255, 255, 255, 0.025));
  border: 1px solid var(--moddin-line-soft, rgba(255, 255, 255, 0.08));
  border-left: 3px solid var(--moddin-accent, #7aa2f7);
  border-radius: 6px;
  padding: 0.55rem 0.85rem;
  margin-bottom: 0.85rem;
  font-size: 0.85rem;
}

.ai-game-banner-icon {
  font-size: 1.05rem;
  color: var(--moddin-accent, #7aa2f7);
}

.ai-game-banner-body strong {
  display: block;
  font-size: 0.85rem;
  font-weight: 600;
  color: var(--moddin-text-soft, #c5cad3);
}

.ai-game-banner-body p {
  margin: 0.1rem 0 0;
  color: var(--moddin-text-muted, #9aa3b2);
  font-size: 0.78rem;
  line-height: 1.4;
}

.ai-game-banner-actions {
  display: flex;
  align-items: center;
  gap: 0.35rem;
}

.ai-game-banner-primary {
  background: transparent;
  border: 1px solid var(--moddin-accent, #7aa2f7);
  color: var(--moddin-accent, #7aa2f7);
  border-radius: 4px;
  padding: 0.3rem 0.7rem;
  font-size: 0.78rem;
  font-weight: 500;
  cursor: pointer;
}

.ai-game-banner-primary:hover {
  background: rgba(122, 162, 247, 0.12);
}

.ai-game-banner-secondary {
  background: transparent;
  border: 1px solid var(--moddin-line-soft, rgba(255, 255, 255, 0.12));
  color: var(--moddin-text-muted, #9aa3b2);
  border-radius: 4px;
  padding: 0.3rem 0.7rem;
  font-size: 0.78rem;
  cursor: pointer;
}

.ai-game-banner-secondary:hover {
  border-color: var(--moddin-text-soft, #c5cad3);
  color: var(--moddin-text-soft, #c5cad3);
}

.ai-game-banner-dismiss {
  background: transparent;
  border: 1px solid transparent;
  color: var(--moddin-text-muted, #9aa3b2);
  width: 24px;
  height: 24px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.9rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.ai-game-banner-dismiss:hover {
  color: inherit;
  background: var(--moddin-surface-3, rgba(255, 255, 255, 0.05));
}
</style>
