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
  gap: 0.75rem;
  background: linear-gradient(
    90deg,
    rgba(122, 162, 247, 0.18) 0%,
    rgba(122, 162, 247, 0.06) 100%
  );
  border: 1px solid var(--moddin-accent, #7aa2f7);
  border-radius: 8px;
  padding: 0.75rem 1rem;
  margin-bottom: 1rem;
}

.ai-game-banner-icon {
  font-size: 1.4rem;
}

.ai-game-banner-body strong {
  display: block;
  font-size: 0.95rem;
  color: var(--moddin-text, #e8ecf2);
}

.ai-game-banner-body p {
  margin: 0.2rem 0 0;
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.82rem;
}

.ai-game-banner-actions {
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.ai-game-banner-primary {
  background: var(--moddin-accent, #7aa2f7);
  color: #0c0e15;
  border: none;
  border-radius: 6px;
  padding: 0.45rem 0.9rem;
  font-weight: 600;
  cursor: pointer;
}

.ai-game-banner-secondary {
  background: transparent;
  border: 1px solid var(--moddin-accent, #7aa2f7);
  color: var(--moddin-accent, #7aa2f7);
  border-radius: 6px;
  padding: 0.4rem 0.85rem;
  font-size: 0.82rem;
  cursor: pointer;
}

.ai-game-banner-dismiss {
  background: transparent;
  border: 1px solid var(--moddin-border, #3a4252);
  color: inherit;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  cursor: pointer;
  font-size: 1rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
</style>
