<script setup lang="ts">
import { onMounted, ref, useId, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from './AppIcon.vue'
import { useDialogLifecycle } from '../../composables/useDialogLifecycle'

withDefaults(defineProps<{
  title: string
  eyebrow?: string
  description?: string
  size?: 'md' | 'lg'
}>(), { size: 'md' })

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()

const titleId = useId()

// Every dialog in the app is mounted only while it is open, so its mount
// *is* its open. Routing the focus handling through the shared lifecycle
// is what makes a keyboard user land in the same place in all of them.
const open = ref(true)
const { dialogElement, openDialog, closeDialog } = useDialogLifecycle(open)

onMounted(() => {
  void openDialog()
})

// The lifecycle owns Escape, so the emit is derived from the state it sets
// rather than from the call site — otherwise the key would close a ref the
// parent never reads and the dialog would simply stay on screen.
watch(open, (isOpen) => {
  if (!isOpen) emit('close')
})
</script>

<template>
  <div class="dialog-backdrop" @click.self="closeDialog">
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
        <button class="btn btn-icon" type="button" :aria-label="t('close')" @click="closeDialog">
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
