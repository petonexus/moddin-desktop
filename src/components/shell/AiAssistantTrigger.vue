<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import AppIcon from '../ui/AppIcon.vue'
import { useAiAssistantTrigger } from '../../composables/useAiAssistant'
const { t } = useI18n()
const { openFor } = useAiAssistantTrigger()
function open() { openFor({ mode: 'author' }) }
</script>
<template>
  <button class="nav-item" type="button" @click="open">
    <AppIcon name="ai" />
    <span>{{ t('aiAssistantButton') }}</span>
  </button>
</template>
