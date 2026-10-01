<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import AppIcon from '../ui/AppIcon.vue'

/**
 * The AI entry point next to the primary game action.
 *
 * This used to be a split button with a dropdown of four more verbs for
 * the same dialog — recommend, audit, diagnose, contribute — and the
 * sidebar had a fifth. Five names for one destination means the user has
 * to guess which one they want, and "Contribute" fired a window event
 * nothing listened to. It is one button now, and the dialog's own mode
 * picker is the only place the modes are named.
 *
 * The component stays dumb about what the dialog does with the request.
 */
const emit = defineEmits<{ ask: [] }>()

const { t } = useI18n()
</script>

<template>
  <button class="ai-menu-pill" type="button" @click="emit('ask')">
    <AppIcon name="ai" :size="14" />
    <span>{{ t('aiTopbarAsk') }}</span>
  </button>
</template>

<style scoped>
.ai-menu-pill {
  display: inline-flex;
  align-items: center;
  gap: var(--moddin-space-2);
  background: transparent;
  color: var(--moddin-text-soft);
  border: 1px solid var(--moddin-line);
  border-radius: var(--moddin-radius-pill);
  padding: 6px var(--moddin-space-3);
  font: inherit;
  font-size: var(--moddin-text-md);
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  transition: background var(--moddin-fast) var(--moddin-ease), border-color var(--moddin-fast) var(--moddin-ease), color var(--moddin-fast) var(--moddin-ease);
}
.ai-menu-pill:hover {
  background: var(--moddin-surface-2);
  border-color: var(--moddin-accent-ring);
  color: inherit;
}
.ai-menu-pill:focus-visible {
  outline: 2px solid var(--moddin-accent);
  outline-offset: 2px;
}
</style>
