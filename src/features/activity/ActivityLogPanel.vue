<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import BaseDialog from '../../components/ui/BaseDialog.vue'
import ConfirmDialog from '../../components/ui/ConfirmDialog.vue'
import ErrorCallout from '../../components/ui/ErrorCallout.vue'
import EmptyState from '../../components/ui/EmptyState.vue'
import { useFriendlyError, type FriendlyErrorRule } from '../../composables/useFriendlyError'
import { useBackendText } from '../../composables/useBackendText'
import { dateLocaleFor } from '../../i18n/locale'
import { activityCopyForLocale } from './copy'
import { useActivityLogPanel } from './useActivityLogPanel'

const { t, locale } = useI18n()
const copy = computed(() => activityCopyForLocale(locale.value))
const { text } = useBackendText()

const {
  open,
  loading,
  clearing,
  error,
  logs,
  search,
  level,
  expanded,
  filteredLogs,
  toggleDetails,
  refresh,
  openPanel,
  closePanel,
  clearLogs: clearLogsAction,
} = useActivityLogPanel({ actionLabel: (entry) => text(entry.action) })
const pendingClear = ref(false)
const hasFilters = computed(() => Boolean(search.value.trim()) || level.value !== 'all')
const resultsLabel = computed(() => copy.value.results
  .replace('{count}', String(filteredLogs.value.length))
  .replace('{total}', String(logs.value.length)))

function resetFilters() {
  search.value = ''
  level.value = 'all'
}

function formatDate(timestamp: number) {
  return new Intl.DateTimeFormat(dateLocaleFor(locale.value), {
    dateStyle: 'short',
    timeStyle: 'medium',
  }).format(new Date(timestamp))
}

function levelBadge(level: string) {
  if (level === 'success') return 'badge-success'
  if (level === 'error') return 'badge-danger'
  if (level === 'warning') return 'badge-warning'
  return 'badge-info'
}

function onToggle(id: string, event: Event) {
  const isOpen = (event.target as HTMLDetailsElement).open
  if (isOpen !== expanded.value.has(id)) toggleDetails(id)
}

/**
 * Raw backend errors turned into a summary plus a 'why' line the user
 * can act on, with the original kept one disclosure away for power users.
 */
const errorRules = computed<FriendlyErrorRule[]>(() => {
  const c = copy.value
  return [
    { match: /permission|denied|access|os error 5|0x80070005|being used|locked|sharing/i, title: c.errorLockedTitle, why: c.errorLockedWhy, showRaw: true },
    { match: /not found|os error 2|no such file|cannot find/i, title: c.errorMissingTitle, why: c.errorMissingWhy, showRaw: false },
    { match: /disk full|no space|os error 112|0x80070027|0x80070070/i, title: c.errorDiskTitle, why: c.errorDiskWhy, showRaw: true },
    { title: c.errorGenericTitle, why: c.errorGenericWhy, showRaw: true },
  ]
})

const friendlyError = useFriendlyError({ error, rules: () => errorRules.value })

async function clearLogs() {
  await clearLogsAction()
  pendingClear.value = false
}
</script>

<template>
  <button class="nav-item" type="button" aria-haspopup="dialog" :aria-expanded="open" @click="openPanel">
    <AppIcon name="activity" />
    <span>{{ copy.button }}</span>
  </button>

  <Teleport to="body">
    <BaseDialog
      v-if="open"
      size="lg"
      :title="copy.title"
      :description="copy.subtitle"
      :busy="clearing"
      @close="closePanel"
    >
      <div class="activity-toolbar">
        <!--
          UX-27: a placeholder is not a label, and the level `<select>` had
          neither. These are the two controls that decide which entries
          are on screen, and they were the two a screen-reader user could
          not name. `.sr-only` labels, the same shape the game list uses.
        -->
        <label class="sr-only" for="activity-search">{{ copy.searchLabel }}</label>
        <input
          id="activity-search"
          v-model="search"
          class="input"
          type="search"
          :placeholder="copy.search"
        />
        <label class="sr-only" for="activity-level">{{ copy.levelLabel }}</label>
        <select id="activity-level" v-model="level" class="select">
          <option value="all">{{ copy.all }}</option>
          <option value="success">{{ copy.success }}</option>
          <option value="error">{{ copy.error }}</option>
          <option value="warning">{{ copy.warning }}</option>
          <option value="info">{{ copy.info }}</option>
        </select>
        <button class="btn btn-sm" :class="{ 'is-loading': loading }" type="button" :disabled="loading || clearing" :aria-busy="loading" @click="refresh">
          <AppIcon v-if="!loading" name="refresh" :size="14" />
          {{ loading ? copy.refreshing : copy.refresh }}
        </button>
        <button v-if="hasFilters" class="btn btn-sm activity-reset" type="button" @click="resetFilters">{{ copy.resetFilters }}</button>
      </div>

      <p class="activity-results" role="status" aria-live="polite">{{ loading ? copy.loading : resultsLabel }}</p>

      <ErrorCallout :error="friendlyError" />
      <EmptyState v-if="loading && logs.length === 0" busy :description="copy.loading" />
      <EmptyState
        v-else-if="filteredLogs.length === 0"
        icon="activity"
        :title="logs.length ? copy.filteredEmptyTitle : undefined"
        :description="logs.length ? copy.filteredEmptyHint : copy.empty"
      >
        <button v-if="logs.length && hasFilters" class="btn btn-primary btn-sm activity-reset" type="button" @click="resetFilters">{{ copy.resetFilters }}</button>
      </EmptyState>

      <div v-else class="activity-list" :aria-busy="loading || clearing">
        <article v-for="entry in filteredLogs" :key="entry.id" class="activity-entry" :class="`level-${entry.level}`">
          <div class="activity-entry-top">
            <span class="badge" :class="levelBadge(entry.level)">{{ copy[entry.level] }}</span>
            <strong>{{ text(entry.action) }}</strong>
            <time :datetime="new Date(entry.timestamp).toISOString()">{{ formatDate(entry.timestamp) }}</time>
          </div>
          <p>{{ entry.message }}</p>

          <details
            v-if="Object.keys(entry.details).length || entry.transactionId || entry.gameId"
            class="disclosure activity-details"
            :open="expanded.has(entry.id)"
            @toggle="onToggle(entry.id, $event)"
          >
            <summary>{{ copy.details }}</summary>
            <dl>
              <div v-if="entry.gameId"><dt>{{ copy.game }}</dt><dd>{{ entry.gameId }}</dd></div>
              <div v-if="entry.transactionId"><dt>{{ copy.transaction }}</dt><dd>{{ entry.transactionId }}</dd></div>
              <div v-for="(value, key) in entry.details" :key="key"><dt>{{ key }}</dt><dd>{{ value }}</dd></div>
            </dl>
          </details>
        </article>
      </div>

      <template #footer>
        <span class="footer-hint">{{ copy.storage }}</span>
        <button class="btn btn-danger btn-sm" :class="{ 'is-loading': clearing }" type="button" :disabled="loading || clearing || logs.length === 0" :aria-busy="clearing" @click="pendingClear = true">{{ clearing ? copy.clearing : copy.clear }}</button>
      </template>
    </BaseDialog>
    <ConfirmDialog
      v-if="pendingClear"
      :title="copy.clearConfirmTitle"
      :description="copy.confirmClear"
      :confirm-label="copy.clear"
      :cancel-label="t('cancel')"
      :busy="clearing"
      @close="pendingClear = false"
      @confirm="clearLogs"
    />
  </Teleport>
</template>

<style scoped src="./activity-log-panel.css"></style>
