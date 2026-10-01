<script setup lang="ts">
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

const { t } = useI18n()
</script>

<template>
  <!--
    Global utilities render as sidebar navigation items. Each feature still
    owns its trigger and its dialog; the shell only decides where they sit.
    The two groups answer "what am I changing" vs "what can help me", so a
    player scanning the sidebar can tell them apart before reading either.
  -->
  <div class="nav-group">
    <span class="nav-label">{{ t('navToolsMods') }}</span>
    <nav class="nav-section" :aria-label="t('navToolsMods')">
      <OpenXrManager />
      <CommunityPanel />
      <CollectionsPanel />
      <ProfilesPanel />
      <ContributeTrigger />
    </nav>

    <span class="nav-label">{{ t('navToolsHelp') }}</span>
    <nav class="nav-section" :aria-label="t('navToolsHelp')">
      <AiAssistantTrigger />
      <LocalAiPanel />
      <!--
        The updater sits with the tools rather than the mods because it is
        about the app itself. It is an entry, not a banner: nothing checks
        for an update until the user opens it and asks.
      -->
      <AppUpdatePanel />
      <ActivityLogPanel />
    </nav>
  </div>
</template>
