<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { activityCopyForLocale, activityDateLocale } from '../features/activity/copy'
import { useActivityLogPanel } from '../features/activity/useActivityLogPanel'

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

  <div v-if="open" class="activity-backdrop" @click.self="open = false">
    <section class="activity-panel" role="dialog" aria-modal="true" :aria-label="copy.title">
      <header class="activity-header">
        <div>
          <small>ACTIVITY</small>
          <h2>{{ copy.title }}</h2>
          <p>{{ copy.subtitle }}</p>
        </div>
        <button class="activity-icon-button" type="button" :aria-label="copy.close" @click="open = false">×</button>
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

<style scoped>
.activity-fab {
  position: fixed;
  right: 122px;
  bottom: 22px;
  z-index: 1500;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border: 1px solid rgba(148, 163, 184, 0.25);
  border-radius: 999px;
  color: #e5eefc;
  background: rgba(15, 23, 42, 0.94);
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.32);
  cursor: pointer;
  font: inherit;
  font-weight: 700;
}

.activity-fab span { color: #93c5fd; font-size: 1.15rem; }

.activity-backdrop {
  position: fixed;
  inset: 0;
  z-index: 2100;
  display: grid;
  place-items: center;
  padding: 28px;
  background: rgba(2, 6, 23, 0.78);
  backdrop-filter: blur(8px);
}

.activity-panel {
  width: min(940px, 96vw);
  max-height: 90vh;
  overflow: auto;
  border: 1px solid rgba(148, 163, 184, 0.2);
  border-radius: 18px;
  background: #0f172a;
  color: #e5eefc;
  box-shadow: 0 30px 80px rgba(0, 0, 0, 0.48);
  padding: 22px;
}

.activity-header,
.activity-toolbar,
.activity-entry-top,
.activity-entry-title {
  display: flex;
  align-items: center;
  gap: 12px;
}

.activity-header { justify-content: space-between; }
.activity-header small { color: #94a3b8; letter-spacing: .08em; }
.activity-header h2 { margin: 3px 0 2px; }
.activity-header p { margin: 0; color: #94a3b8; }
.activity-icon-button { border: 0; background: transparent; color: #cbd5e1; font-size: 1.8rem; cursor: pointer; }

.activity-toolbar { margin: 20px 0 8px; }
.activity-toolbar input { flex: 1; min-width: 180px; }
.activity-toolbar input,
.activity-toolbar select {
  padding: 9px 11px;
  border: 1px solid rgba(148, 163, 184, 0.22);
  border-radius: 9px;
  color: #e5eefc;
  background: #111c31;
}

.activity-secondary,
.activity-danger,
.activity-link {
  border-radius: 9px;
  padding: 8px 11px;
  font: inherit;
  font-weight: 700;
  cursor: pointer;
}
.activity-secondary { border: 1px solid rgba(148, 163, 184, .24); background: rgba(30, 41, 59, .78); color: #dbeafe; }
.activity-danger { border: 1px solid rgba(239, 68, 68, .35); background: rgba(127, 29, 29, .25); color: #fecaca; }
.activity-link { border: 0; background: transparent; color: #93c5fd; padding: 4px 0; }
.activity-secondary:disabled,
.activity-danger:disabled { opacity: .45; cursor: not-allowed; }

.activity-storage { margin: 0 0 14px; color: #64748b; font-size: .78rem; }
.activity-error,
.activity-empty { padding: 14px; border: 1px solid rgba(148, 163, 184, .16); border-radius: 10px; color: #cbd5e1; }
.activity-error { color: #fca5a5; border-color: rgba(239, 68, 68, .28); }
.activity-list { display: grid; gap: 9px; }
.activity-entry { padding: 13px 14px; border: 1px solid rgba(148, 163, 184, .14); border-left-width: 3px; border-radius: 10px; background: rgba(30, 41, 59, .38); }
.activity-entry.level-success { border-left-color: #22c55e; }
.activity-entry.level-error { border-left-color: #ef4444; }
.activity-entry.level-warning { border-left-color: #f59e0b; }
.activity-entry.level-info { border-left-color: #3b82f6; }
.activity-entry-top { justify-content: space-between; }
.activity-entry-title { min-width: 0; flex-wrap: wrap; }
.activity-entry-title time { color: #64748b; font-size: .76rem; }
.activity-level { padding: 2px 7px; border-radius: 999px; background: rgba(148, 163, 184, .12); color: #cbd5e1; font-size: .7rem; font-weight: 700; text-transform: uppercase; }
.activity-entry p { margin: 7px 0 0; color: #cbd5e1; overflow-wrap: anywhere; }
.activity-details { display: grid; gap: 5px; margin-top: 10px; padding-top: 10px; border-top: 1px solid rgba(148, 163, 184, .12); }
.activity-details > div { display: grid; grid-template-columns: minmax(100px, 160px) 1fr; gap: 10px; }
.activity-details span { color: #94a3b8; font-size: .76rem; }
.activity-details code { color: #bfdbfe; overflow-wrap: anywhere; }

@media (max-width: 760px) {
  .activity-fab { right: 112px; }
  .activity-backdrop { padding: 12px; }
  .activity-panel { padding: 16px; }
  .activity-toolbar { align-items: stretch; flex-direction: column; }
  .activity-toolbar input,
  .activity-toolbar select,
  .activity-secondary,
  .activity-danger { width: 100%; }
  .activity-entry-top { align-items: flex-start; }
}

/* Keep utility surfaces in the same visual language as the main workspace. */
.activity-fab {
  right: 22px;
  bottom: 74px;
  border-color: #2b3744;
  color: #e8edf3;
  background: rgba(16, 23, 32, 0.96);
}

.activity-fab span { color: #b3a8ff; }
.activity-backdrop { background: rgba(3, 6, 10, 0.76); }
.activity-panel { border-color: #2b3744; background: #101720; color: #e8edf3; }
.activity-header p,
.activity-header small,
.activity-storage,
.activity-entry-title time,
.activity-details span { color: #8996a5; }
.activity-toolbar input,
.activity-toolbar select { border-color: #2b3744; color: #e8edf3; background: #0b1118; }
.activity-secondary { border-color: #394756; color: #e8edf3; background: #18222d; }
.activity-link { color: #b3a8ff; }
.activity-entry { border-color: #2a3642; background: rgba(20, 29, 39, 0.8); }
.activity-entry p { color: #c6d0dc; }
.activity-details code { color: #c5bcff; }

@media (max-width: 760px) {
  .activity-fab { right: 16px; bottom: 70px; }
}
</style>
