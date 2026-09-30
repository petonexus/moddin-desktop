<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import BaseDialog from '../../components/ui/BaseDialog.vue'
import ErrorCallout from '../../components/ui/ErrorCallout.vue'
import EmptyState from '../../components/ui/EmptyState.vue'
import { useFriendlyError, type FriendlyErrorRule } from '../../composables/useFriendlyError'
import { dateLocaleFor } from '../../i18n/locale'
import { activityCopyForLocale } from './copy'
import { useActivityLogPanel } from './useActivityLogPanel'

const { locale } = useI18n()
const copy = computed(() => activityCopyForLocale(locale.value))

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
} = useActivityLogPanel()

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
  await clearLogsAction(copy.value.confirmClear)
}
</script>

<template>
  <button class="nav-item" type="button" @click="openPanel">
    <AppIcon name="activity" />
    <span>{{ copy.button }}</span>
  </button>

  <Teleport to="body">
    <BaseDialog
      v-if="open"
      size="lg"
      :title="copy.title"
      :description="copy.subtitle"
      @close="closePanel"
    >
      <div class="activity-toolbar">
        <input v-model="search" class="input" type="search" :placeholder="copy.search" />
        <select v-model="level" class="select">
          <option value="all">{{ copy.all }}</option>
          <option value="success">{{ copy.success }}</option>
          <option value="error">{{ copy.error }}</option>
          <option value="warning">{{ copy.warning }}</option>
          <option value="info">{{ copy.info }}</option>
        </select>
        <button class="btn btn-sm" type="button" :disabled="loading" @click="refresh">
          <AppIcon name="refresh" :size="14" />
          {{ copy.refresh }}
        </button>
      </div>

      <ErrorCallout :error="friendlyError" />
      <EmptyState v-if="loading && logs.length === 0" busy />
      <EmptyState
        v-else-if="filteredLogs.length === 0"
        icon="activity"
        :description="copy.empty"
      />

      <div v-else class="activity-list">
        <article v-for="entry in filteredLogs" :key="entry.id" class="activity-entry" :class="`level-${entry.level}`">
          <div class="activity-entry-top">
            <span class="badge" :class="levelBadge(entry.level)">{{ copy[entry.level] }}</span>
            <strong>{{ entry.action }}</strong>
            <time>{{ formatDate(entry.timestamp) }}</time>
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
        <button class="btn btn-danger btn-sm" type="button" :disabled="clearing || logs.length === 0" @click="clearLogs">{{ copy.clear }}</button>
      </template>
    </BaseDialog>
  </Teleport>
</template>

<style scoped src="./activity-log-panel.css"></style>
