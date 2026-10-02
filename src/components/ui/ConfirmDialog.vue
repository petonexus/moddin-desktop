<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import AppIcon from './AppIcon.vue'
import BaseDialog from './BaseDialog.vue'

/**
 * Confirmation for an action that changes or removes something.
 *
 * Moddin is built on "every change is previewed, backed up and
 * reversible", and the actions that are hardest to undo were the ones
 * firing straight from a button. This is the one place that pattern is
 * enforced, so a new destructive action does not have to remember to
 * build its own dialog.
 *
 * Buttons are rendered cancel-first, and the confirm button is never
 * autofocused: a dialog opening over a game folder should not remove a
 * mod because the user pressed Enter.
 */
withDefaults(
  defineProps<{
    title: string
    description?: string
    confirmLabel: string
    cancelLabel?: string
    /** `danger` for removal, `default` for a plain yes/no. */
    tone?: 'default' | 'danger'
    /** Lines the user should read before agreeing — affected paths,
     *  what stops working, how to get it back. */
    details?: string[]
    /** Shown under the details, usually pointing at History. */
    footnote?: string
    busy?: boolean
  }>(),
  { tone: 'danger', cancelLabel: '', details: () => [], busy: false },
)

const emit = defineEmits<{ close: []; confirm: [] }>()
const { t } = useI18n()
</script>

<template>
  <BaseDialog
    :title="title"
    :description="description"
    :busy="busy"
    @close="emit('close')"
  >
    <ul v-if="details.length" class="confirm-details">
      <li v-for="(detail, index) in details" :key="index">{{ detail }}</li>
    </ul>
    <p v-if="footnote" class="confirm-footnote">{{ footnote }}</p>

    <template #footer>
      <button
        class="btn"
        type="button"
        v-bind="{ 'data-dialog-initial-focus': '' }"
        :disabled="busy"
        :aria-label="cancelLabel || t('actionCancel')"
        @click="emit('close')"
      >
        {{ cancelLabel || t('actionCancel') }}
      </button>
      <button
        class="btn"
        :class="[tone === 'danger' ? 'btn-danger-solid' : 'btn-primary', { 'is-loading': busy }]"
        type="button"
        :disabled="busy"
        :aria-busy="busy || undefined"
        @click="emit('confirm')"
      >
        <AppIcon v-if="tone === 'danger' && !busy" name="alert" :size="16" />
        {{ confirmLabel }}
      </button>
    </template>
  </BaseDialog>
</template>
