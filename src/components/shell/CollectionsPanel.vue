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
import type { CollectionBlocker, CollectionSummary } from '../../types/collection'

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

/**
 * Collections the loader refused for the selected game, with the reason.
 *
 * The verdict is the backend's, not a rule re-derived here: the loader
 * resolves the engine gate the same way `capability_list` does, so this
 * list cannot claim a set is fine that the capability cards would hide,
 * or the reverse. A collection curated for another game stays in the
 * list — the AI assistant recommends across games, and a user with two
 * games in their library can see what the other one gets — but the wizard
 * will not open for it.
 */
const blockedRows = computed(() => {
  if (selectedGame.value.gameId === null) return []
  return install.collections.value
    .map((entry) => ({ entry, blockers: entry.preset?.blocked ?? [] }))
    .filter((row) => row.blockers.length > 0)
})

function blockerText(entry: CollectionSummary, blocker: CollectionBlocker) {
  const game = selectedGame.value.gameName ?? selectedGame.value.gameId ?? ''
  if (blocker.kind === 'wrongGame') {
    return t('collectionBlockedWrongGame', { target: blocker.targetGame ?? '', game })
  }
  const member = entry.preset?.capabilities.find((capability) => capability.id === blocker.capabilityId)
  return t('collectionBlockedEngineMismatch', {
    capability: member?.displayName ?? blocker.capabilityId ?? '',
    supported: blocker.supportedEngines.join(', '),
    game,
  })
}

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
  // A set the loader already refused must not reach the wizard. The
  // wizard installs every member in order, so the refusal would arrive
  // halfway through the run with files already written, instead of here
  // where the reason is already on screen.
  if (selectedGame.value.gameId !== null && (summary.preset?.blocked ?? []).length > 0) return
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
      <div v-if="blockedRows.length > 0" class="collections-blocked" role="status">
        <p class="collections-blocked-heading">
          {{ t('collectionBlockedHeading', { game: selectedGame.gameName ?? selectedGame.gameId ?? '' }) }}
        </p>
        <ul>
          <li v-for="row in blockedRows" :key="row.entry.id">
            <strong>{{ row.entry.displayName }}</strong>
            <span v-for="(blocker, index) in row.blockers" :key="index">
              {{ blockerText(row.entry, blocker) }}
            </span>
          </li>
        </ul>
      </div>

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

<style scoped>
/*
 * The refused sets sit above the list rather than inside a row: the row
 * is owned by `CollectionsTab`, and a reason that changes the meaning of
 * an entire list does not belong to any one entry. Quiet styling on
 * purpose — this is a note about two of six rows, not an error.
 */
.collections-blocked {
  display: grid;
  gap: var(--moddin-space-2);
  margin-bottom: var(--moddin-space-4);
  padding: var(--moddin-space-3) var(--moddin-space-4);
  border: 1px solid var(--moddin-line-soft);
  border-radius: var(--moddin-radius-md);
  background: var(--moddin-surface-2);
}

.collections-blocked-heading {
  margin: 0;
  color: var(--moddin-text-soft);
  font-size: var(--moddin-text-sm);
}

.collections-blocked ul { margin: 0; padding-left: var(--moddin-space-4); }
.collections-blocked li { font-size: var(--moddin-text-sm); color: var(--moddin-text-muted); }
.collections-blocked strong { color: var(--moddin-text); margin-right: var(--moddin-space-2); }
</style>
