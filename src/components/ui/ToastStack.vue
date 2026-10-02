<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from './AppIcon.vue'

const SUCCESS_TIMEOUT_MS = 6000

const props = defineProps<{ success: string | null; error: string | null }>()
const emit = defineEmits<{ 'dismiss-success': []; 'dismiss-error': [] }>()
const { t } = useI18n()

let timer: number | null = null
let startedAt = 0
let remaining = SUCCESS_TIMEOUT_MS
let hovered = false
let focused = false

function clearTimer() {
  if (timer !== null) window.clearTimeout(timer)
  timer = null
}

function pauseTimer() {
  if (timer !== null) remaining = Math.max(0, remaining - (Date.now() - startedAt))
  clearTimer()
}

function resumeTimer() {
  if (!props.success || timer !== null || hovered || focused || document.hidden) return
  startedAt = Date.now()
  timer = window.setTimeout(() => {
    timer = null
    emit('dismiss-success')
  }, remaining)
}

function setHovered(value: boolean) {
  hovered = value
  if (value) pauseTimer()
  else resumeTimer()
}

function onFocusOut(event: FocusEvent) {
  if (event.currentTarget instanceof HTMLElement && event.relatedTarget instanceof Node
    && event.currentTarget.contains(event.relatedTarget)) return
  focused = false
  resumeTimer()
}

function onFocusIn() {
  focused = true
  pauseTimer()
}

function dismissSuccess() {
  clearTimer()
  emit('dismiss-success')
}

function onVisibilityChange() {
  if (document.hidden) pauseTimer()
  else resumeTimer()
}

// Success messages confirm something the user just did; they should not
// linger. Errors stay until dismissed because they usually need reading.
watch(() => props.success, (value) => {
  clearTimer()
  remaining = SUCCESS_TIMEOUT_MS
  if (!value) {
    hovered = false
    focused = false
  }
  resumeTimer()
}, { immediate: true })

onMounted(() => document.addEventListener('visibilitychange', onVisibilityChange))
onUnmounted(() => {
  clearTimer()
  document.removeEventListener('visibilitychange', onVisibilityChange)
})
</script>

<template>
  <!--
    UX-29: the container carried `aria-live="polite"` and so did the two
    toasts inside it, so every message was announced twice — once as a
    live-region change and once as the `role="alert"` / `role="status"`
    element. The roles are the precise mechanism: `alert` interrupts
    for an error, `status` waits its turn for a success. The container
    is now silent and only carries the stack's layout.
  -->
  <div class="toast-stack">
    <TransitionGroup name="toast">
      <div v-if="error" key="error" class="toast toast-error" role="alert">
        <AppIcon class="toast-icon" name="alert" :size="18" />
        <div>
          <strong>{{ t('toastError') }}</strong>
          <p>{{ error }}</p>
        </div>
        <button class="btn btn-icon" type="button" :aria-label="t('dismiss')" @click="emit('dismiss-error')">
          <AppIcon name="close" />
        </button>
      </div>
      <div
        v-if="success"
        key="success"
        class="toast toast-success"
        role="status"
        @pointerenter="setHovered(true)"
        @pointerleave="setHovered(false)"
        @focusin="onFocusIn"
        @focusout="onFocusOut"
      >
        <AppIcon class="toast-icon" name="check" :size="18" />
        <div>
          <strong>{{ t('toastSuccess') }}</strong>
          <p>{{ success }}</p>
        </div>
        <button class="btn btn-icon" type="button" :aria-label="t('dismiss')" @click="dismissSuccess">
          <AppIcon name="close" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toast-stack {
  position: fixed;
  z-index: 200;
  right: var(--moddin-space-6);
  bottom: var(--moddin-space-6);
  display: grid;
  gap: var(--moddin-space-2);
  width: min(420px, calc(100vw - 32px));
}

.toast {
  display: flex;
  align-items: flex-start;
  gap: var(--moddin-space-3);
  border: 1px solid var(--moddin-line-strong);
  border-radius: var(--moddin-radius-md);
  padding: var(--moddin-space-3) var(--moddin-space-2) var(--moddin-space-3) var(--moddin-space-4);
  background: var(--moddin-surface-2);
  box-shadow: var(--moddin-shadow-md);
}
.toast > div { flex: 1; min-width: 0; }
.toast strong { display: block; font-size: var(--moddin-text-md); }
.toast p { margin-top: 2px; color: var(--moddin-text-soft); font-size: var(--moddin-text-md); overflow-wrap: anywhere; }
.toast-icon { margin-top: 1px; }
.toast .btn-icon { width: 36px; min-height: 36px; margin-top: -2px; }
.toast-success { border-color: var(--moddin-success-line); }
.toast-success .toast-icon { color: var(--moddin-success); }
.toast-error { border-color: var(--moddin-danger-line); }
.toast-error .toast-icon { color: var(--moddin-danger); }

.toast-enter-active, .toast-leave-active { transition: opacity var(--moddin-normal) var(--moddin-ease), transform var(--moddin-normal) var(--moddin-ease); }
.toast-enter-from, .toast-leave-to { opacity: 0; transform: translateY(8px); }
</style>
