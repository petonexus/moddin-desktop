<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../ui/AppIcon.vue'
import ContributeDialog from './ContributeDialog.vue'

/**
 * Sidebar entry point for "help me make a mod".
 *
 * The trigger owns the dialog and opens it directly. An earlier version
 * dispatched a `moddin:open-contribute` window event for the topbar to
 * catch; nothing was listening, so the button did nothing. One owner,
 * one click, no bus.
 */
const { t } = useI18n()
const open = ref(false)
</script>

<template>
  <button class="nav-item" type="button" @click="open = true">
    <AppIcon name="ai" />
    <span>{{ t('contributeButton') }}</span>
  </button>

  <Teleport to="body">
    <ContributeDialog v-if="open" @close="open = false" />
  </Teleport>
</template>
