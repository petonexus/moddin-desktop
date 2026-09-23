<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import AiTopbarMenu from './AiTopbarMenu.vue'

defineProps<{
  detected: number
  supported: number
  loading: boolean
  selectedAppId: string | null
  selectedGameName: string | null
  hasError: boolean
}>()

const emit = defineEmits<{
  rescan: []
  ask: []
  recommend: []
  audit: []
  diagnose: []
  contribute: []
}>()

const { t } = useI18n()
</script>

<template>
  <header class="topbar">
    <div>
      <p class="eyebrow">{{ t('localLibrary') }}</p>
      <h1>{{ t('installedGames') }}</h1>
      <p class="subtle">{{ t('detectedSupported', { detected, supported }) }}</p>
    </div>
    <div class="topbar-actions">
      <button class="secondary-button" :class="{ 'is-loading': loading }" :disabled="loading" @click="emit('rescan')">
        {{ loading ? t('scanning') : t('rescanLibraries') }}
      </button>
      <AiTopbarMenu :selected-app-id="selectedAppId" :selected-game-name="selectedGameName" :has-error="hasError" @ask="emit('ask')" @recommend="emit('recommend')" @audit="emit('audit')" @diagnose="emit('diagnose')" @contribute="emit('contribute')" />
    </div>
  </header>
</template>
