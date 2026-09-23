<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useAiModuleActions } from '../../composables/useAiModuleActions'

const { t } = useI18n()
const aiActions = useAiModuleActions()

function askAiToDiagnose() {
  aiActions.openDiagnoseWithAi({
    message: t('libraryEmptyNoGamesBody'),
    gameId: null,
    gameName: null,
    capabilityId: null,
  })
}
</script>

<template>
  <div class="empty-state empty-state-ai">
    <strong>{{ t('libraryEmptyNoGames') }}</strong>
    <p>{{ t('libraryEmptyNoGamesBody') }}</p>
    <button
      class="secondary-button compact"
      type="button"
      @click="askAiToDiagnose"
    >
      <span aria-hidden="true">✨</span>
      {{ t('libraryEmptyAskAi') }}
    </button>
  </div>
</template>
