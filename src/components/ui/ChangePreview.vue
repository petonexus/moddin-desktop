<script setup lang="ts">
/*
 * Shared "what will happen" block for every install/launch preview, so all
 * dialogs explain changes, warnings and the backup guarantee the same way.
 */
import { useI18n } from 'vue-i18n'
import AppIcon from './AppIcon.vue'

withDefaults(defineProps<{
  changes: string[]
  warnings?: string[]
  location?: string | null
  emptyText?: string
  showBackupNote?: boolean
}>(), { warnings: () => [], location: null, emptyText: undefined, showBackupNote: true })

const { t } = useI18n()
</script>

<template>
  <section class="dialog-section">
    <h3>{{ t('previewWhatHappens') }}</h3>
    <ul v-if="changes.length" class="change-list">
      <li v-for="change in changes" :key="change">
        <AppIcon name="check" :size="14" />
        <span>{{ change }}</span>
      </li>
    </ul>
    <p v-else class="text-muted">{{ emptyText ?? t('previewNothingSafe') }}</p>
  </section>

  <section v-if="warnings.length" class="dialog-section">
    <div class="callout callout-warning">
      <AppIcon class="callout-icon" name="alert" />
      <div>
        <strong>{{ t('previewBeforeYouContinue') }}</strong>
        <ul class="note-list">
          <li v-for="warning in warnings" :key="warning">{{ warning }}</li>
        </ul>
      </div>
    </div>
  </section>

  <slot />

  <p v-if="showBackupNote" class="backup-note">
    <AppIcon name="shield" :size="14" />
    <span>{{ t('previewBackupNote') }}</span>
  </p>

  <details v-if="location" class="disclosure location-disclosure">
    <summary>{{ t('previewWhere') }}</summary>
    <p class="path-text">{{ location }}</p>
  </details>
</template>
