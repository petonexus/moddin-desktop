<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

/**
 * Compact AI menu that sits next to the primary game action. The
 * single button opens a dropdown; the most-used action (Ask AI) is
 * also a click on the button body itself.
 *
 * The component is dumb about *what* each action does — it just emits
 * events up to App.vue so the wiring stays in one place.
 */
const props = defineProps<{
  selectedAppId: string | null
  selectedGameName: string | null
  hasError: boolean
}>()

const emit = defineEmits<{
  ask: []
  recommend: []
  audit: []
  diagnose: []
  contribute: []
}>()

const { t } = useI18n()
const open = ref(false)
const root = ref<HTMLElement | null>(null)

function toggle() {
  open.value = !open.value
}

function close() {
  open.value = false
}

function onDocClick(event: MouseEvent) {
  if (!open.value) return
  const el = root.value
  if (!el) return
  if (event.target instanceof Node && el.contains(event.target)) return
  close()
}

function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape' && open.value) close()
}

onMounted(() => {
  document.addEventListener('click', onDocClick)
  document.addEventListener('keydown', onKey)
})

onUnmounted(() => {
  document.removeEventListener('click', onDocClick)
  document.removeEventListener('keydown', onKey)
})

function recommendLabel(): string {
  if (props.selectedAppId) {
    return t('aiTopbarRecommend', { game: props.selectedGameName ?? '' })
  }
  return t('aiTopbarRecommendGeneric')
}

function onAsk() {
  close()
  emit('ask')
}

function onRecommend() {
  if (!props.selectedAppId) return
  close()
  emit('recommend')
}

function onAudit() {
  close()
  emit('audit')
}

function onDiagnose() {
  if (!props.hasError) return
  close()
  emit('diagnose')
}

function onContribute() {
  close()
  emit('contribute')
}
</script>

<template>
  <div ref="root" class="ai-menu">
    <button
      class="ai-menu-trigger"
      type="button"
      :aria-label="t('aiTopbarAsk')"
      :aria-haspopup="true"
      :aria-expanded="open"
      @click="onAsk"
      @contextmenu.prevent="toggle"
    >
      <span aria-hidden="true" class="ai-menu-sparkle">✨</span>
      <span class="ai-menu-label">{{ t('aiTopbarAsk') }}</span>
      <span
        aria-hidden="true"
        class="ai-menu-caret"
        role="button"
        tabindex="-1"
        @click.stop="toggle"
      >▾</span>
    </button>
    <div v-if="open" class="ai-menu-dropdown" role="menu" :aria-label="t('aiTopbarMenu')">
      <button
        class="ai-menu-item"
        type="button"
        role="menuitem"
        :disabled="!props.selectedAppId"
        @click="onRecommend"
      >
        <span class="ai-menu-item-glyph" aria-hidden="true">🎯</span>
        <span>{{ recommendLabel() }}</span>
      </button>
      <button
        class="ai-menu-item"
        type="button"
        role="menuitem"
        @click="onAudit"
      >
        <span class="ai-menu-item-glyph" aria-hidden="true">🔍</span>
        <span>{{ t('aiTopbarAudit') }}</span>
      </button>
      <button
        class="ai-menu-item"
        type="button"
        role="menuitem"
        :disabled="!props.hasError"
        :title="props.hasError ? undefined : t('aiTopbarDiagnoseHint')"
        @click="onDiagnose"
      >
        <span class="ai-menu-item-glyph" aria-hidden="true">⚠️</span>
        <span>{{ t('aiTopbarDiagnose') }}</span>
      </button>
      <hr class="ai-menu-sep" aria-hidden="true" />
      <button
        class="ai-menu-item"
        type="button"
        role="menuitem"
        @click="onContribute"
      >
        <span class="ai-menu-item-glyph" aria-hidden="true">📝</span>
        <span>{{ t('aiTopbarContribute') }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.ai-menu {
  position: relative;
  display: inline-flex;
  isolation: isolate;
}

.ai-menu-trigger {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: transparent;
  color: var(--moddin-text-soft, #c5cad3);
  border: 1px solid var(--moddin-line, rgba(255, 255, 255, 0.12));
  border-radius: 999px;
  padding: 6px 10px 6px 12px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
}
.ai-menu-trigger:hover {
  background: var(--moddin-surface-2, rgba(255, 255, 255, 0.04));
  border-color: rgba(122, 162, 247, 0.45);
  color: inherit;
}
.ai-menu-trigger:focus-visible {
  outline: 2px solid rgba(122, 162, 247, 0.6);
  outline-offset: 2px;
}

.ai-menu-sparkle { font-size: 13px; }
.ai-menu-label { white-space: nowrap; }

.ai-menu-caret {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  margin-left: 2px;
  border-radius: 4px;
  font-size: 11px;
  color: var(--moddin-text-muted, #9aa3b2);
  cursor: pointer;
}
.ai-menu-caret:hover { background: rgba(255, 255, 255, 0.06); }

.ai-menu-dropdown {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: 50;
  min-width: 260px;
  background: var(--moddin-surface, #15171c);
  border: 1px solid var(--moddin-line, rgba(255, 255, 255, 0.12));
  border-radius: 10px;
  padding: 6px;
  display: grid;
  gap: 2px;
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.4);
}

.ai-menu-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border: 0;
  background: transparent;
  color: inherit;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  text-align: left;
  width: 100%;
}
.ai-menu-item:hover:not(:disabled) {
  background: var(--moddin-surface-2, rgba(255, 255, 255, 0.05));
}
.ai-menu-item:disabled {
  color: var(--moddin-text-muted, #9aa3b2);
  cursor: not-allowed;
}
.ai-menu-item-glyph { width: 18px; text-align: center; }

.ai-menu-sep {
  border: 0;
  height: 1px;
  background: var(--moddin-line-soft, rgba(255, 255, 255, 0.08));
  margin: 4px 6px;
}
</style>
