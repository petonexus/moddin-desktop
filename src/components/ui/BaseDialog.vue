<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, useId } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from './AppIcon.vue'

withDefaults(defineProps<{
  title: string
  eyebrow?: string
  description?: string
  size?: 'md' | 'lg'
}>(), { size: 'md' })

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()

const titleId = useId()
const dialogElement = ref<HTMLElement | null>(null)
let opener: HTMLElement | null = null

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'

function focusable() {
  return Array.from(dialogElement.value?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [])
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    emit('close')
    return
  }
  if (event.key !== 'Tab') return
  const items = focusable()
  if (!items.length) return
  const first = items[0]
  const last = items[items.length - 1]
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault()
    last.focus()
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault()
    first.focus()
  }
}

onMounted(async () => {
  opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
  window.addEventListener('keydown', onKeydown)
  await nextTick()
  // Focus the dialog itself so screen readers announce the title first.
  dialogElement.value?.focus()
})

onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown)
  opener?.focus()
})
</script>

<template>
  <div class="dialog-backdrop" @click.self="emit('close')">
    <section
      ref="dialogElement"
      class="dialog"
      :class="`dialog-${size}`"
      role="dialog"
      aria-modal="true"
      :aria-labelledby="titleId"
      tabindex="-1"
    >
      <header class="dialog-header">
        <div>
          <p v-if="eyebrow" class="overline">{{ eyebrow }}</p>
          <h2 :id="titleId">{{ title }}</h2>
          <p v-if="description" class="dialog-description">{{ description }}</p>
        </div>
        <button class="btn btn-icon" type="button" :aria-label="t('close')" @click="emit('close')">
          <AppIcon name="close" :size="18" />
        </button>
      </header>

      <div class="dialog-body">
        <slot />
      </div>

      <footer v-if="$slots.footer" class="dialog-footer">
        <slot name="footer" />
      </footer>
    </section>
  </div>
</template>
