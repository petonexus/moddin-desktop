<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import AppIcon from './AppIcon.vue'
import type { FriendlyError } from '../../composables/useFriendlyError'

defineProps<{ error: FriendlyError | null }>()

const { t } = useI18n()
</script>

<template>
  <div v-if="error" class="callout callout-danger" role="alert">
    <AppIcon class="callout-icon" name="alert" :size="18" />
    <div class="error-content">
      <strong v-if="error.title">{{ error.title }}</strong>
      <p>{{ error.why }}</p>
      <details v-if="error.showRaw" class="callout-raw">
        <summary>{{ t('errorRawToggle') }}</summary>
        <code>{{ error.raw }}</code>
      </details>
    </div>
  </div>
</template>

<style scoped>
.error-content { flex: 1; min-width: 0; overflow-wrap: anywhere; }
</style>
