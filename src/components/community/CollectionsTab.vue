<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import AppIcon from '../ui/AppIcon.vue'
import ErrorCallout from '../ui/ErrorCallout.vue'
import EmptyState from '../ui/EmptyState.vue'
import type { FriendlyError } from '../../composables/useFriendlyError'
import type { CollectionSummary } from '../../types/collection'

/**
 * The collections list, with nothing else in it.
 *
 * Self-contained on purpose: it takes the catalog and renders it, and
 * emits `install` for a chosen row. Every string, empty state and error
 * surface is shared with the rest of the app, so mounting it inside the
 * Community panel later is a wiring change and not a rewrite.
 */
withDefaults(
  defineProps<{
    collections: CollectionSummary[]
    loading?: boolean
    /** Already shaped by `useFriendlyError`; this only renders it. */
    error?: FriendlyError | null
    /** Id of the row whose install dialog is open, so its button can wait. */
    pendingId?: string | null
  }>(),
  { loading: false, error: null, pendingId: null },
)

const emit = defineEmits<{ install: [CollectionSummary] }>()

const { t } = useI18n()
</script>

<template>
  <div class="collections-tab">
    <ErrorCallout :error="error" />

    <EmptyState v-if="loading" busy :description="t('collectionLoading')" />

    <EmptyState
      v-else-if="collections.length === 0"
      icon="community"
      :description="t('collectionEmpty')"
    />

    <div v-else class="collections-list">
      <article v-for="entry in collections" :key="entry.id" class="collections-entry">
        <div class="collections-entry-top">
          <strong>{{ entry.displayName || entry.id }}</strong>
          <span v-if="entry.targetGame" class="badge">{{ entry.targetGame }}</span>
        </div>

        <p v-if="entry.description" class="collections-description">{{ entry.description }}</p>

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

        <div class="collections-entry-actions">
          <button
            class="btn btn-primary btn-sm"
            :class="{ 'is-loading': pendingId === entry.id }"
            type="button"
            :disabled="pendingId === entry.id || !entry.preset"
            :title="entry.preset ? undefined : t('collectionErrorNoPresetWhy')"
            @click="emit('install', entry)"
          >
            <AppIcon v-if="pendingId !== entry.id" name="arrow-up" :size="14" />
            {{ t('collectionInstall') }}
          </button>
        </div>
      </article>
    </div>
  </div>
</template>

<style scoped>
.collections-list { display: grid; gap: var(--moddin-space-2); }

.collections-entry {
  display: grid;
  gap: var(--moddin-space-3);
  border: 1px solid var(--moddin-line-soft);
  border-radius: var(--moddin-radius-md);
  padding: var(--moddin-space-3) var(--moddin-space-4);
  background: var(--moddin-surface-2);
}

.collections-entry-top { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--moddin-space-3); }
.collections-entry-top strong { display: block; font-size: var(--moddin-text-md); }

.collections-description { margin: 0; color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }

.collections-meta {
  display: flex;
  flex-wrap: wrap;
  gap: var(--moddin-space-4);
  margin: 0;
  color: var(--moddin-text-muted);
  font-size: var(--moddin-text-sm);
}
.collections-meta strong { color: var(--moddin-text); }

.collections-entry-actions { display: flex; justify-content: flex-end; }
</style>
