<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import BaseDialog from '../../components/ui/BaseDialog.vue'
import ChangePreview from '../../components/ui/ChangePreview.vue'
import type { DesktopShortcutPreview } from './types'

defineProps<{
  moduleName: string
  preview: DesktopShortcutPreview
  busy: boolean
  disabled: boolean
}>()

const emit = defineEmits<{
  close: []
  confirm: []
}>()

const { t } = useI18n()
</script>

<template>
  <BaseDialog :eyebrow="t('previewEyebrow')" :title="moduleName" @close="emit('close')">
    <ChangePreview
      :changes="[preview.willReplace ? t('desktopShortcutWillReplace') : t('desktopShortcutWillCreate')]"
      :location="preview.shortcutPath"
    />
    <template #footer>
      <button class="btn" type="button" @click="emit('close')">{{ t('cancel') }}</button>
      <button
        class="btn btn-primary"
        :class="{ 'is-loading': busy }"
        type="button"
        :disabled="!preview.canApply || busy || disabled"
        @click="emit('confirm')"
      >
        {{ busy ? t('previewWorking') : t('actionCreateShortcut') }}
      </button>
    </template>
  </BaseDialog>
</template>
