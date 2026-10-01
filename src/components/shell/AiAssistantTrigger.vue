<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import AppIcon from '../ui/AppIcon.vue'
import { useAiAssistantTrigger } from '../../composables/useAiAssistant'

const { t } = useI18n()
const { openFor } = useAiAssistantTrigger()

// One verb, same as the topbar: open the assistant and let its mode
// picker decide what happens. The sidebar used to open `author`, which
// was the least discoverable of the four and the least likely to be what
// someone reaching for "IA" wanted.
function open() {
  void openFor({ mode: 'author', gameId: null, gameName: null, intent: '' })
}
</script>
<template>
  <button class="nav-item" type="button" @click="open">
    <AppIcon name="ai" />
    <span>{{ t('aiTopbarAsk') }}</span>
  </button>
</template>
