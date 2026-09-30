<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../ui/AppIcon.vue'
import BaseDialog from '../ui/BaseDialog.vue'
import CollectionsTab from '../community/CollectionsTab.vue'
import CollectionInstallDialog from '../community/CollectionInstallDialog.vue'
import { useCollectionInstall } from '../../composables/useCollectionInstall'
import { useFriendlyError, type FriendlyErrorRule } from '../../composables/useFriendlyError'
import { readLocalValue } from '../../services/storage'
import { findCatalogGameById } from '../../services/catalog'
import type { CollectionSummary } from '../../types/collection'

/**
 * Sidebar panel that lists collections and hands a chosen one to the
 * install dialog.
 *
 * Mounted from `GlobalTools.vue` next to the Community panel rather than
 * inside it: the two show the same catalog from different angles, and
 * this way the collections UI reaches the screen without a tab wiring
 * change inside a panel that belongs to another surface. `CollectionsTab`
 * itself takes props and emits, so folding it in as a tab later is a
 * template change and nothing more.
 */
const { t } = useI18n()
const open = ref(false)
const pending = ref<CollectionSummary | null>(null)

const install = useCollectionInstall()

const selectedGame = computed(() => {
  const gameId = readLocalValue('moddin-selected-appId')
  if (!gameId) return { gameId: null, gameName: null }
  return { gameId, gameName: findCatalogGameById(gameId)?.name ?? null }
})

const collectionsError = computed<string | null>(() =>
  install.collectionsError.value === null ? null : install.collectionsError.value,
)

/**
 * Read through a computed rather than inline: the list dialog is behind
 * a `v-if="!pending"`, so the template narrows `pending` to `null` and
 * `pending?.id` there would have nothing to read.
 */
const pendingId = computed(() => pending.value?.id ?? null)

const errorRules = computed<FriendlyErrorRule[]>(() => [
  { context: 'load', title: t('collectionErrorLoadTitle'), why: t('collectionErrorLoadWhy'), showRaw: true },
  { title: t('collectionErrorGenericTitle'), why: t('collectionErrorGenericWhy'), showRaw: true },
])

const friendlyError = useFriendlyError({
  error: collectionsError,
  rules: () => errorRules.value,
  context: install.collectionsErrorContext,
})

async function openPanel() {
  open.value = true
  await install.refreshCollections()
}

function closePanel() {
  open.value = false
  pending.value = null
}

function choose(summary: CollectionSummary) {
  pending.value = summary
}

/**
 * The run finished. Refresh the list behind the dialog, but leave the
 * dialog up: the completed state — every step, every transaction — is
 * the answer to "did it work", and yanking it away to show the list
 * again would throw that away.
 */
async function finished() {
  await install.refreshCollections()
}

// Transient state is cleared when the panel leaves the screen, whichever
// way it left — button, Escape or backdrop.
watch(open, (isOpen) => {
  if (isOpen) return
  install.reset()
  install.collectionsError.value = null
})
</script>

<template>
  <button class="nav-item" type="button" @click="openPanel">
    <AppIcon name="library" />
    <span>{{ t('collectionButton') }}</span>
  </button>

  <Teleport to="body">
    <!--
      One dialog at a time. The install wizard replaces the list rather
      than stacking on it, because every `BaseDialog` registers its own
      global Escape handler: two open at once would make a single Escape
      close both, and the user would lose the list they came from.
    -->
    <BaseDialog
      v-if="open && !pending"
      :title="t('collectionTitle')"
      :eyebrow="t('collection')"
      :description="t('collectionSubtitle')"
      size="lg"
      @close="closePanel"
    >
      <CollectionsTab
        :collections="install.collections.value"
        :loading="install.collectionsLoading.value"
        :error="friendlyError"
        :pending-id="pendingId"
        @install="choose"
      />

      <template #footer>
        <button class="btn" type="button" @click="closePanel">
          {{ t('close') }}
        </button>
      </template>
    </BaseDialog>

    <CollectionInstallDialog
      v-if="pending"
      :summary="pending"
      :game-id="selectedGame.gameId"
      :game-name="selectedGame.gameName"
      @close="pending = null"
      @installed="finished"
    />
  </Teleport>
</template>
