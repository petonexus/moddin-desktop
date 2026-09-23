<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useCollectionInstall } from '../../composables/useCollectionInstall'
import type { CollectionSpec } from '../../types/collection'
import CollectionInstallDialog from './CollectionInstallDialog.vue'

const { t } = useI18n()
const install = useCollectionInstall()

const dialogSpec = ref<CollectionSpec | null>(null)
const dialogOpen = ref(false)
const gameContext = ref({
  gameId: '',
  gameName: '',
  installDir: '',
  executableDir: '',
})

function readSelectedAppId(): string | null {
  try {
    return window.localStorage.getItem('moddin-selected-appId')
  } catch {
    return null
  }
}

async function openInstallDialog(summary: { id: string; targetGame: string | null; displayName: string }) {
  const gameId = readSelectedAppId() ?? summary.targetGame ?? 'unknown'
  gameContext.value = {
    gameId,
    gameName: summary.displayName,
    // For now we let the backend derive install/executable paths from
    // the game id; the collection command tolerates empty strings.
    installDir: '',
    executableDir: '',
  }
  try {
    dialogSpec.value = await install.loadDetail(summary.id)
    dialogOpen.value = true
  } catch (err) {
    install.collectionsError.value = err instanceof Error ? err.message : String(err)
  }
}

function closeDialog() {
  dialogOpen.value = false
  dialogSpec.value = null
  install.reset()
}

function originLabel(origin: string): string {
  if (origin === 'builtIn') return t('collectionOriginBuiltIn')
  if (origin === 'local') return t('collectionOriginLocal')
  return t('collectionOriginCommunity')
}

onMounted(async () => {
  await install.refreshCollections()
})
</script>

<template>
  <div class="collections-tab">
    <div v-if="install.collectionsError.value" class="collections-error" role="alert">
      {{ install.collectionsError.value }}
    </div>

    <div v-if="install.collectionsLoading.value" class="collections-empty">
      {{ t('collectionLoading') }}
    </div>

    <div
      v-else-if="install.collections.value.length === 0"
      class="collections-empty"
    >
      {{ t('collectionEmpty') }}
    </div>

    <div v-else class="collections-list">
      <article
        v-for="entry in install.collections.value"
        :key="entry.id"
        class="collections-entry"
      >
        <div class="collections-entry-top">
          <div>
            <strong>{{ entry.displayName }}</strong>
            <span class="collections-entry-id">
              {{ entry.id }} · v{{ entry.targetGame ?? '*' }}
            </span>
          </div>
          <span class="collections-origin-tag">{{ originLabel(entry.origin) }}</span>
        </div>

        <p v-if="entry.description" class="collections-description">
          {{ entry.description }}
        </p>

        <p class="collections-meta">
          <span>
            <strong>{{ entry.capabilityCount }}</strong>
            {{ t('collectionCapabilityCount', { count: entry.capabilityCount }) }}
          </span>
          <span>
            <strong>{{ entry.requiredCount }}</strong>
            {{ t('collectionRequiredCount', { count: entry.requiredCount }) }}
          </span>
        </p>

        <button class="collections-primary" type="button" @click="openInstallDialog(entry)">
          {{ t('collectionInstall') }}
        </button>
      </article>
    </div>

    <CollectionInstallDialog
      :open="dialogOpen"
      :spec="dialogSpec"
      :game-context="gameContext"
      @close="closeDialog"
    />
  </div>
</template>

<style scoped>
.collections-tab {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}

.collections-error {
  background: rgba(255, 90, 90, 0.12);
  border: 1px solid rgba(255, 90, 90, 0.4);
  color: #ff8c8c;
  padding: 0.5rem 0.7rem;
  border-radius: 6px;
  font-size: 0.85rem;
}

.collections-empty {
  text-align: center;
  padding: 1.25rem;
  color: var(--moddin-muted, #8c93a3);
}

.collections-list {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}

.collections-entry {
  background: var(--moddin-surface-2, #1a1f29);
  border: 1px solid var(--moddin-border, #3a4252);
  border-radius: 8px;
  padding: 0.7rem 0.9rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.collections-entry-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
}

.collections-entry-id {
  display: block;
  font-size: 0.78rem;
  color: var(--moddin-muted, #8c93a3);
  margin-top: 0.1rem;
  font-family: 'Cascadia Code', 'Consolas', monospace;
}

.collections-description {
  margin: 0;
  font-size: 0.85rem;
  color: var(--moddin-text, #e8ecf2);
}

.collections-meta {
  display: flex;
  gap: 1rem;
  margin: 0;
  font-size: 0.78rem;
  color: var(--moddin-muted, #8c93a3);
}

.collections-meta strong {
  color: var(--moddin-text, #e8ecf2);
  font-size: 0.9rem;
}

.collections-origin-tag {
  padding: 0.05rem 0.5rem;
  border-radius: 999px;
  font-size: 0.7rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  border: 1px solid var(--moddin-border, #3a4252);
  color: var(--moddin-muted, #8c93a3);
  background: var(--moddin-surface, #141823);
}

.collections-primary {
  align-self: flex-end;
  background: var(--moddin-accent, #7aa2f7);
  color: #0c0e15;
  border: none;
  border-radius: 6px;
  padding: 0.4rem 1rem;
  font-weight: 600;
  cursor: pointer;
}
</style>
