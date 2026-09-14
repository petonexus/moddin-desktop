<script setup lang="ts">
import { useI18n } from 'vue-i18n'
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
  <div class="modal-backdrop" @click.self="emit('close')">
    <section class="modal-card">
      <div class="modal-heading">
        <div>
          <p class="eyebrow">{{ t('desktopShortcutPreview') }}</p>
          <h2>{{ moduleName }}</h2>
        </div>
        <button class="icon-button" :aria-label="t('close')" @click="emit('close')">×</button>
      </div>

      <div class="preview-summary">
        <div>
          <span>{{ t('desktopShortcutPath') }}</span>
          <strong>{{ preview.shortcutPath }}</strong>
        </div>
        <div>
          <span>{{ t('executable') }}</span>
          <strong>{{ preview.targetPath }}</strong>
        </div>
        <div>
          <span>{{ t('desktopShortcutIcon') }}</span>
          <strong>{{ preview.iconPath }}</strong>
        </div>
      </div>

      <div class="preview-block">
        <h3>{{ t('changes') }}</h3>
        <ul>
          <li>{{ preview.willReplace ? t('desktopShortcutWillReplace') : t('desktopShortcutWillCreate') }}</li>
        </ul>
      </div>

      <div class="modal-actions">
        <button class="secondary-button" @click="emit('close')">{{ t('cancel') }}</button>
        <button
          class="primary-button"
          :class="{ 'is-loading': busy }"
          :disabled="!preview.canApply || busy || disabled"
          @click="emit('confirm')"
        >
          {{ busy ? t('applying') : t('desktopShortcutCreate') }}
        </button>
      </div>
    </section>
  </div>
</template>
