<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../ui/AppIcon.vue'
import { useAiAssistantTrigger } from '../../composables/useAiAssistant'

const props = defineProps<{
  gameId: string | null
  gameName: string | null
}>()

const { t } = useI18n()
const { openAiAssistantRecommendations } = useAiAssistantTrigger()

const dismissed = ref(false)

const gameLabel = computed(() => props.gameName ?? props.gameId ?? t('aiAssistantGameBannerAnyGame'))
const isVisible = computed(() => Boolean(props.gameId) && !dismissed.value)

// One verb here too. The banner used to offer "audit" and "recommend"
// side by side, which asked the user to choose a mode before they had
// even said what they wanted; the dialog's picker does that better.
async function ask() {
  await openAiAssistantRecommendations({
    gameId: props.gameId,
    gameName: props.gameName,
    intent: t('aiAssistantGameBannerIntent', { game: gameLabel.value }),
  })
}

function dismiss() {
  dismissed.value = true
}
</script>

<template>
  <aside v-if="isVisible" class="ai-game-banner">
    <div class="ai-game-banner-icon" aria-hidden="true">
      <AppIcon name="ai" :size="18" />
    </div>
    <div class="ai-game-banner-body">
      <strong>{{ t('aiAssistantGameBannerHeading') }}</strong>
      <p>{{ t('aiAssistantGameBannerBody', { game: gameLabel }) }}</p>
    </div>
    <div class="ai-game-banner-actions">
      <button class="ai-game-banner-primary" type="button" @click="ask">
        {{ t('aiAssistantGameBannerAsk') }}
      </button>
      <button
        class="ai-game-banner-dismiss"
        type="button"
        :aria-label="t('aiAssistantGameBannerDismiss')"
        @click="dismiss"
      >
        <AppIcon name="close" :size="14" />
      </button>
    </div>
  </aside>
</template>

<style scoped>
.ai-game-banner {
  display: grid;
  grid-template-columns: auto 1fr auto;
  align-items: center;
  gap: var(--moddin-space-4);
  background: var(--moddin-surface-2);
  border: 1px solid var(--moddin-line-soft);
  border-left: 3px solid var(--moddin-accent);
  border-radius: var(--moddin-radius-sm);
  padding: var(--moddin-space-2) var(--moddin-space-4);
  margin-bottom: var(--moddin-space-4);
  font-size: var(--moddin-text-md);
}

.ai-game-banner-icon {
  color: var(--moddin-accent);
}

.ai-game-banner-body strong {
  display: block;
  font-size: var(--moddin-text-md);
  font-weight: 600;
  color: var(--moddin-text-soft);
}

.ai-game-banner-body p {
  margin: 2px 0 0;
  color: var(--moddin-text-muted);
  font-size: var(--moddin-text-sm);
  line-height: 1.4;
}

.ai-game-banner-actions {
  display: flex;
  align-items: center;
  gap: var(--moddin-space-1);
}

.ai-game-banner-primary {
  background: transparent;
  border: 1px solid var(--moddin-accent);
  color: var(--moddin-accent);
  border-radius: var(--moddin-radius-sm);
  padding: var(--moddin-space-1) var(--moddin-space-3);
  font-size: var(--moddin-text-sm);
  font-weight: 500;
  cursor: pointer;
}

.ai-game-banner-primary:hover {
  background: var(--moddin-accent-soft);
}

.ai-game-banner-dismiss {
  background: transparent;
  border: 1px solid transparent;
  color: var(--moddin-text-muted);
  width: 24px;
  height: 24px;
  border-radius: var(--moddin-radius-sm);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.ai-game-banner-dismiss:hover {
  color: inherit;
  background: var(--moddin-surface-3);
}
</style>
