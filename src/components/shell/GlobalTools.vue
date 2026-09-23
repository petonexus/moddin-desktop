<script setup lang="ts">
import ActivityLogPanel from '../../features/activity/ActivityLogPanel.vue'
import CommunityPanel from '../../features/community/CommunityPanel.vue'
import OpenXrManager from '../../features/openxr/OpenXrManager.vue'
</script>

<template>
  <!--
    Global utilities render as sidebar navigation items. Each feature still
    owns its trigger and its dialog; the shell only decides where they sit.
  -->
  <OpenXrManager />
  <CommunityPanel />
  <ActivityLogPanel />
</template>
