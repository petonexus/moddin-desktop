<script setup lang="ts">
import ActivityLogPanel from '../../features/activity/ActivityLogPanel.vue'
import AiAssistantDialog from './AiAssistantDialog.vue'
import AiAssistantTrigger from './AiAssistantTrigger.vue'
import CommunityPanel from '../community/CommunityPanel.vue'
import ContributeTrigger from './ContributeTrigger.vue'
import OpenXrManager from '../../features/openxr/OpenXrManager.vue'
</script>

<template>
  <!--
    Global utilities intentionally live outside the primary App workspace.
    The dock owns trigger positioning; each feature still owns its dialog.
  -->
  <div class="global-tools-dock">
    <AiAssistantTrigger />
    <ContributeTrigger />
    <ActivityLogPanel />
    <CommunityPanel />
    <OpenXrManager />
  </div>
  <AiAssistantDialog />
</template>

<style scoped src="./global-tools.css"></style>
