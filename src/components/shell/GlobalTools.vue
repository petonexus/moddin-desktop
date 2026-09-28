<script setup lang="ts">
import ActivityLogPanel from '../../features/activity/ActivityLogPanel.vue'
import AiAssistantTrigger from './AiAssistantTrigger.vue'
import CommunityPanel from '../../features/community/CommunityPanel.vue'
import LocalAiPanel from '../../features/local-ai/LocalAiPanel.vue'
import OpenXrManager from '../../features/openxr/OpenXrManager.vue'
</script>

<template>
  <!--
    Global utilities render as sidebar navigation items. Each feature still
    owns its trigger and its dialog; the shell only decides where they sit.
  -->
  <!-- The shell decides placement; the AI feature owns the trigger behavior. -->
  <AiAssistantTrigger />
  <OpenXrManager />
  <LocalAiPanel />
  <CommunityPanel />
  <ActivityLogPanel />
</template>
