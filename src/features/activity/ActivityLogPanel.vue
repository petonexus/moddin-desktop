<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { activityCopyForLocale, activityDateLocale } from './copy'
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
  return new Intl.DateTimeFormat(activityDateLocale(locale.value), {
    dateStyle: 'short',
    timeStyle: 'medium',
  }).format(new Date(timestamp))
}

async function clearLogs() {
  await clearLogsAction(copy.value.confirmClear)
}
</script>

<template>
  <button class="activity-fab" type="button" @click="openPanel">
    <span>≡</span>
    {{ copy.button }}
  </button>

  <div v-if="open" class="activity-backdrop" @click.self="closePanel">
    <section class="activity-panel" role="dialog" aria-modal="true" :aria-label="copy.title">
      <header class="activity-header">
        <div>
          <small>ACTIVITY</small>
          <h2>{{ copy.title }}</h2>
          <p>{{ copy.subtitle }}</p>
        </div>
        <button class="activity-icon-button" type="button" :aria-label="copy.close" @click="closePanel">×</button>
      </header>

      <div class="activity-toolbar">
        <input v-model="search" type="search" :placeholder="copy.search" />
        <select v-model="level">
          <option value="all">{{ copy.all }}</option>
          <option value="success">{{ copy.success }}</option>
          <option value="error">{{ copy.error }}</option>
          <option value="warning">{{ copy.warning }}</option>
          <option value="info">{{ copy.info }}</option>
        </select>
        <button class="activity-secondary" type="button" :disabled="loading" @click="refresh">{{ copy.refresh }}</button>
        <button class="activity-danger" type="button" :disabled="clearing || logs.length === 0" @click="clearLogs">{{ copy.clear }}</button>
      </div>

      <p class="activity-storage">{{ copy.storage }}</p>
      <div v-if="error" class="activity-error">{{ error }}</div>
      <div v-if="loading && logs.length === 0" class="activity-empty">{{ copy.refresh }}…</div>
      <div v-else-if="filteredLogs.length === 0" class="activity-empty">{{ copy.empty }}</div>

      <div v-else class="activity-list">
        <article v-for="entry in filteredLogs" :key="entry.id" class="activity-entry" :class="`level-${entry.level}`">
          <div class="activity-entry-top">
            <div class="activity-entry-title">
              <span class="activity-level">{{ copy[entry.level] }}</span>
              <strong>{{ entry.action }}</strong>
              <time>{{ formatDate(entry.timestamp) }}</time>
            </div>
            <button
              v-if="Object.keys(entry.details).length || entry.transactionId || entry.gameId"
              class="activity-link"
              type="button"
              @click="toggleDetails(entry.id)"
            >
              {{ copy.details }}
            </button>
          </div>

          <p>{{ entry.message }}</p>

          <div v-if="expanded.has(entry.id)" class="activity-details">
            <div v-if="entry.gameId"><span>{{ copy.game }}</span><code>{{ entry.gameId }}</code></div>
            <div v-if="entry.transactionId"><span>{{ copy.transaction }}</span><code>{{ entry.transactionId }}</code></div>
            <div v-for="(value, key) in entry.details" :key="key"><span>{{ key }}</span><code>{{ value }}</code></div>
          </div>
        </article>
      </div>
    </section>
  </div>
</template>

<style scoped src="./activity-log-panel.css"></style>
