<script setup lang="ts">
import ActivityLogPanel from '../../features/activity/ActivityLogPanel.vue'
import OpenXrManager from '../../features/openxr/OpenXrManager.vue'
</script>

<template>
  <!--
    Global utilities intentionally live outside the primary App workspace.
    Keeping them behind a shell boundary makes it possible to move them into a
    proper command bar/settings area later without coupling them to a game view.
  -->
  <ActivityLogPanel />
  <OpenXrManager />
</template>
