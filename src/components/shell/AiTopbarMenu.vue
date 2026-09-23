<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

/**
 * Split-button IA toolbar that sits at the top-right of the library
 * topbar. The main half triggers the contextual default action
 * (currently "audit"); the arrow half opens a dropdown of nearby modes
 * (recommend / audit / diagnose current error / contribute).
 *
 * The component is dumb about *what* each action does — it just emits
 * events up to App.vue so the wiring stays in one place. The component
 * only owns:
 *   - the open/close state of the dropdown,
 *   - the click-outside dismissal,
 *   - the keyboard escape handling,
 *   - the visual styling (split-button + menu).
 *
 * This keeps App.vue from re-bloating past its 115 KB byte budget and
 * lets the menu render identically in any future surface that mounts
 * it.
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
  <div ref="root" class="ai-split">
    <button
      class="ai-split-main"
      type="button"
      :aria-label="t('aiTopbarAsk')"
      @click="onAsk"
    >
      <span aria-hidden="true" class="ai-split-sparkle">✨</span>
      <span class="ai-split-label">{{ t('aiTopbarAsk') }}</span>
    </button>
    <button
      class="ai-split-arrow"
      type="button"
      :aria-label="t('aiTopbarOpenMenu')"
      :aria-expanded="open"
      aria-haspopup="menu"
      @click="toggle"
    >
      <span aria-hidden="true">▾</span>
    </button>
    <div v-if="open" class="ai-split-menu" role="menu" :aria-label="t('aiTopbarMenu')">
      <button
        class="ai-split-item"
        type="button"
        role="menuitem"
        :disabled="!props.selectedAppId"
        @click="onRecommend"
      >
        <span class="ai-split-item-glyph" aria-hidden="true">🎯</span>
        <span>{{ recommendLabel() }}</span>
      </button>
      <button
        class="ai-split-item"
        type="button"
        role="menuitem"
        @click="onAudit"
      >
        <span class="ai-split-item-glyph" aria-hidden="true">🔍</span>
        <span>{{ t('aiTopbarAudit') }}</span>
      </button>
      <button
        class="ai-split-item"
        type="button"
        role="menuitem"
        :disabled="!props.hasError"
        :title="props.hasError ? undefined : t('aiTopbarDiagnoseHint')"
        @click="onDiagnose"
      >
        <span class="ai-split-item-glyph" aria-hidden="true">⚠️</span>
        <span>{{ t('aiTopbarDiagnose') }}</span>
      </button>
      <hr class="ai-split-sep" aria-hidden="true" />
      <button
        class="ai-split-item"
        type="button"
        role="menuitem"
        @click="onContribute"
      >
        <span class="ai-split-item-glyph" aria-hidden="true">📝</span>
        <span>{{ t('aiTopbarContribute') }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.ai-split {
  position: relative;
  display: inline-flex;
  align-items: stretch;
  border-radius: 10px;
  overflow: visible;
  isolation: isolate;
}

.ai-split-main,
.ai-split-arrow {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  background: linear-gradient(135deg, rgba(122, 162, 247, 0.22), rgba(122, 162, 247, 0.08));
  color: var(--moddin-text, #e8ecf2);
  border: 1px solid rgba(122, 162, 247, 0.55);
  font-weight: 600;
  font-size: 0.85rem;
  padding: 0.55rem 0.95rem;
  cursor: pointer;
  transition: background 120ms ease, transform 80ms ease, border-color 120ms ease;
}

.ai-split-main {
  border-right: none;
  border-top-left-radius: 10px;
  border-bottom-left-radius: 10px;
  padding-right: 0.7rem;
}

.ai-split-arrow {
  border-top-right-radius: 10px;
  border-bottom-right-radius: 10px;
  padding: 0.55rem 0.6rem;
  font-size: 0.95rem;
  border-left: 1px solid rgba(122, 162, 247, 0.35);
}

.ai-split-main:hover,
.ai-split-arrow:hover {
  background: linear-gradient(135deg, rgba(122, 162, 247, 0.34), rgba(122, 162, 247, 0.16));
  border-color: rgba(122, 162, 247, 0.8);
}

.ai-split-main:focus-visible,
.ai-split-arrow:focus-visible {
  outline: 2px solid rgba(122, 162, 247, 0.85);
  outline-offset: 2px;
}

.ai-split-main:active,
.ai-split-arrow:active {
  transform: translateY(1px);
}

.ai-split-sparkle {
  font-size: 0.95rem;
}

.ai-split-menu {
  position: absolute;
  top: calc(100% + 8px);
  right: 0;
  min-width: 240px;
  max-width: 320px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 6px;
  border-radius: 10px;
  background: rgba(15, 19, 26, 0.98);
  border: 1px solid rgba(122, 162, 247, 0.35);
  box-shadow: 0 14px 28px rgba(0, 0, 0, 0.45);
  z-index: 50;
}

.ai-split-item {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  background: transparent;
  border: none;
  color: var(--moddin-text, #e8ecf2);
  padding: 0.55rem 0.7rem;
  border-radius: 6px;
  text-align: left;
  font-size: 0.85rem;
  cursor: pointer;
  transition: background 100ms ease;
}

.ai-split-item:hover:not(:disabled) {
  background: rgba(122, 162, 247, 0.16);
}

.ai-split-item:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.ai-split-item-glyph {
  font-size: 1rem;
  flex: 0 0 auto;
}

.ai-split-sep {
  border: none;
  border-top: 1px solid rgba(122, 162, 247, 0.18);
  margin: 4px 6px;
}

@media (max-width: 720px) {
  .ai-split-label {
    /* keep the sparkle visible but tighten the wordmark on narrow viewports */
    display: none;
  }
  .ai-split-main {
    padding: 0.55rem 0.6rem;
  }
}
</style>
