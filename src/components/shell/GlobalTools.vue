<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import ActivityLogPanel from '../../features/activity/ActivityLogPanel.vue'
import AiAssistantTrigger from './AiAssistantTrigger.vue'
import AppUpdatePanel from '../../features/app-update/AppUpdatePanel.vue'
import CollectionsPanel from './CollectionsPanel.vue'
import CommunityPanel from '../../features/community/CommunityPanel.vue'
import ContributeTrigger from './ContributeTrigger.vue'
import LocalAiPanel from '../../features/local-ai/LocalAiPanel.vue'
import OpenXrManager from '../../features/openxr/OpenXrManager.vue'
import ProfilesPanel from '../../features/profiles/ProfilesPanel.vue'
import { globalToolsCopyForLocale } from './global-tools-copy'

const { locale } = useI18n()
const copy = computed(() => globalToolsCopyForLocale(locale.value))
</script>

<template>
  <!--
    Global utilities render as sidebar navigation items. Each feature still
    owns its trigger and its dialog; the shell only decides where they sit.
    Groups follow the player's task: organize mods, manage the app and VR,
    or get help creating a mod.
  -->
  <div class="nav-group">
    <span class="nav-label">{{ copy.organize }}</span>
    <nav class="nav-section" :aria-label="copy.organize">
      <CommunityPanel />
      <CollectionsPanel />
      <ProfilesPanel />
    </nav>

    <span class="nav-label">{{ copy.system }}</span>
    <nav class="nav-section" :aria-label="copy.system">
      <OpenXrManager />
      <!--
        The updater sits with the tools rather than the mods because it is
        about the app itself. It is an entry, not a banner: nothing checks
        for an update until the user opens it and asks.
      -->
      <AppUpdatePanel />
      <ActivityLogPanel />
    </nav>

    <span class="nav-label">{{ copy.create }}</span>
    <nav class="nav-section" :aria-label="copy.create">
      <AiAssistantTrigger />
      <LocalAiPanel />
      <ContributeTrigger />
    </nav>
  </div>
</template>
