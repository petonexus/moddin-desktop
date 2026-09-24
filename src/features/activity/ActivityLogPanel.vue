<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import { dateLocaleFor } from '../../i18n/locale'
import { activityCopyForLocale } from './copy'
import { useActivityLogPanel } from './useActivityLogPanel'

const { locale } = useI18n()
const copy = computed(() => activityCopyForLocale(locale.value))

const {
  open,
  dialogElement,
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
    <div v-if="open" class="dialog-backdrop" @click.self="closePanel">
      <section
        ref="dialogElement"
        class="dialog dialog-lg"
        role="dialog"
        aria-modal="true"
        :aria-label="copy.title"
        tabindex="-1"
      >
        <header class="dialog-header">
          <div>
            <h2>{{ copy.title }}</h2>
            <p class="dialog-description">{{ copy.subtitle }}</p>
          </div>
          <button class="btn btn-icon" type="button" :aria-label="copy.close" @click="closePanel">
            <AppIcon name="close" :size="18" />
          </button>
        </header>

        <div class="dialog-body">
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

          <div v-if="error" class="callout callout-danger">{{ error }}</div>
          <div v-if="loading && logs.length === 0" class="empty-state"><span class="spinner" /></div>
          <div v-else-if="filteredLogs.length === 0" class="empty-state">
            <AppIcon name="activity" :size="28" />
            <span>{{ copy.empty }}</span>
          </div>

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
        </div>

        <footer class="dialog-footer">
          <span class="footer-hint">{{ copy.storage }}</span>
          <button class="btn btn-danger btn-sm" type="button" :disabled="clearing || logs.length === 0" @click="clearLogs">{{ copy.clear }}</button>
        </footer>
      </section>
    </div>
  </Teleport>
</template>

<style scoped src="./activity-log-panel.css"></style>
