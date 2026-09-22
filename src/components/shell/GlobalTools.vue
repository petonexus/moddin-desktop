<script setup lang="ts">
import ActivityLogPanel from '../../features/activity/ActivityLogPanel.vue'
import CommunityPanel from '../community/CommunityPanel.vue'
import OpenXrManager from '../../features/openxr/OpenXrManager.vue'
</script>

<template>
  <!--
    Global utilities intentionally live outside the primary App workspace.
    The dock owns trigger positioning; each feature still owns its dialog.
  -->
  <div class="global-tools-dock">
    <ActivityLogPanel />
    <CommunityPanel />
    <OpenXrManager />
  </div>
</template>

<style scoped src="./global-tools.css"></style>
