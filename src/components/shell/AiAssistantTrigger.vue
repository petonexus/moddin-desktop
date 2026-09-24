<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useAiAssistantTrigger } from '../../composables/useAiAssistant'
import type { AuthorPromptContext } from '../../types/ai-assistant'

const { t } = useI18n()
const { openFor } = useAiAssistantTrigger()

function open() {
  const ctx: AuthorPromptContext = { mode: 'author' }
  openFor(ctx)
}
</script>

<template>
  <button class="ai-fab" type="button" @click="open">
    <span>✨</span>
    {{ t('aiAssistantButton') }}
  </button>
</template>

<style scoped>
.ai-fab {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.5rem 0.9rem;
  border-radius: 999px;
  border: 1px solid var(--moddin-accent, #7aa2f7);
  background: var(--moddin-surface-2, #1a1f29);
  color: var(--moddin-text, #e8ecf2);
  font-size: 0.85rem;
  cursor: pointer;
}

.ai-fab:hover {
  border-color: var(--moddin-accent, #7aa2f7);
  background: var(--moddin-surface, #141823);
}
</style>
