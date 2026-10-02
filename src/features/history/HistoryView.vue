<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import ConfirmDialog from '../../components/ui/ConfirmDialog.vue'
import EmptyState from '../../components/ui/EmptyState.vue'
import type { TransactionRecord } from '../../types/transaction'
import SnapshotPanel from './SnapshotPanel.vue'
import { historyCopyForLocale } from './copy'

const props = defineProps<{
  transactions: TransactionRecord[]
  loading: boolean
  busyId: string | null
  externalBusy?: boolean
  gameName: (gameId: string) => string
  kindLabel: (kind: string) => string
  formatDate: (timestamp: number) => string
  blockedReason: (transaction: TransactionRecord) => string | undefined
}>()

const emit = defineEmits<{ refresh: []; undo: [transaction: TransactionRecord]; 'busy-change': [busy: boolean] }>()
const { t, locale } = useI18n()
const copy = computed(() => historyCopyForLocale(locale.value))

const status = ref<'all' | 'applied' | 'rolled_back'>('all')
const search = ref('')
const snapshotBusy = ref(false)
watch(snapshotBusy, (busy) => emit('busy-change', busy), { flush: 'sync' })
onUnmounted(() => emit('busy-change', false))
const working = computed(() => props.loading || props.busyId !== null || snapshotBusy.value || Boolean(props.externalBusy))
const activeCount = computed(() => props.transactions.filter((item) => item.status === 'applied').length)
const undoneCount = computed(() => props.transactions.filter((item) => item.status === 'rolled_back').length)
const hasFilters = computed(() => status.value !== 'all' || Boolean(search.value.trim()))
const visible = computed(() => {
  const term = search.value.trim().toLocaleLowerCase(locale.value)
  return props.transactions
    .filter((item) => status.value === 'all' || item.status === status.value)
    .filter((item) => !term || [item.label, props.gameName(item.gameId), props.kindLabel(item.kind)]
      .some((value) => value.toLocaleLowerCase(locale.value).includes(term)))
    .slice()
    .sort((a, b) => b.createdAt - a.createdAt)
})
const resultLabel = computed(() => copy.value.results
  .replace('{count}', String(visible.value.length))
  .replace('{total}', String(props.transactions.length)))

function resetFilters() {
  status.value = 'all'
  search.value = ''
}

function changeLabel(transaction: TransactionRecord) {
  return transaction.label || props.kindLabel(transaction.kind)
}

// Undo rewrites files in the game folder. Every row's button is
// labelled "Desfazer", so the list alone does not tell the user what
// they are about to revert — the dialog names it and counts the files.
const pendingUndo = ref<{ transaction: TransactionRecord; label: string; fileCount: number } | null>(null)

function askUndo(transaction: TransactionRecord) {
  if (working.value || props.blockedReason(transaction)) return
  pendingUndo.value = {
    transaction,
    label: changeLabel(transaction),
    fileCount: transaction.files?.length ?? 0,
  }
}

function confirmUndo() {
  const pending = pendingUndo.value
  if (!pending || working.value || props.blockedReason(pending.transaction)) return
  pendingUndo.value = null
  emit('undo', pending.transaction)
}
</script>

<template>
  <section class="page history-page">
    <header class="page-header">
      <div>
        <h1>{{ t('historyTitle') }}</h1>
        <p>{{ t('historySubtitle') }}</p>
      </div>
      <button class="btn history-refresh" :class="{ 'is-loading': loading }" type="button" :disabled="working" :aria-busy="loading" @click="emit('refresh')">
        <AppIcon v-if="!loading" name="refresh" :size="14" />
        {{ loading ? copy.refreshing : t('historyRefresh') }}
      </button>
    </header>

    <!-- Restoring a snapshot changes every transaction it captured, so the
         panel reports the change upward instead of only refreshing itself. -->
    <details class="history-snapshots disclosure">
      <summary>
        <AppIcon name="shield" :size="16" />
        <strong>{{ copy.restorePoints }}</strong>
        <span>{{ copy.restorePointsHint }}</span>
      </summary>
      <SnapshotPanel
        :transactions="transactions"
        :game-name="gameName"
        :format-date="formatDate"
        :external-busy="loading || busyId !== null || externalBusy"
        @busy="snapshotBusy = $event"
        @changed="emit('refresh')"
      />
    </details>

    <div class="history-toolbar">
      <div class="history-search">
        <label class="sr-only" for="history-search">{{ copy.searchLabel }}</label>
        <AppIcon name="search" :size="16" />
        <input id="history-search" v-model="search" class="input" type="search" :placeholder="copy.search" />
      </div>
      <div class="segmented" role="group" :aria-label="copy.filtersLabel">
        <button type="button" :aria-pressed="status === 'all'" @click="status = 'all'">
          {{ t('historyFilterAll') }} <span class="count">{{ transactions.length }}</span>
        </button>
        <button type="button" :aria-pressed="status === 'applied'" @click="status = 'applied'">
          {{ t('historyFilterActive') }} <span class="count">{{ activeCount }}</span>
        </button>
        <button type="button" :aria-pressed="status === 'rolled_back'" @click="status = 'rolled_back'">
          {{ copy.undoneFilter }} <span class="count">{{ undoneCount }}</span>
        </button>
      </div>
      <button v-if="hasFilters" class="btn btn-sm history-reset" type="button" @click="resetFilters">{{ copy.resetFilters }}</button>
    </div>

    <div class="history-results">
      <span role="status" aria-live="polite">{{ loading ? t('historyLoading') : resultLabel }}</span>
      <span>{{ copy.latestFirst }}</span>
    </div>

    <div class="panel history-list" :aria-busy="working">
      <EmptyState
        v-if="loading && !transactions.length"
        busy
        :description="t('historyLoading')"
      />
      <EmptyState
        v-else-if="!visible.length"
        icon="history"
        :title="transactions.length ? copy.filteredEmptyTitle : t('historyEmptyTitle')"
        :description="transactions.length ? copy.filteredEmptyHint : t('historyEmptyHint')"
      >
        <button v-if="transactions.length && hasFilters" class="btn btn-primary btn-sm" type="button" @click="resetFilters">{{ copy.resetFilters }}</button>
      </EmptyState>

      <article v-for="transaction in visible" :key="transaction.id" class="history-row" :class="{ undone: transaction.status === 'rolled_back', pending: transaction.status !== 'applied' && transaction.status !== 'rolled_back' }" :aria-busy="busyId === transaction.id">
        <div class="history-marker" aria-hidden="true">
          <AppIcon :name="transaction.status === 'applied' ? 'check' : transaction.status === 'rolled_back' ? 'undo' : 'info'" :size="14" />
        </div>
        <div class="history-copy">
          <div class="history-title">
            <strong>{{ changeLabel(transaction) }}</strong>
            <span class="badge" :class="transaction.status === 'applied' ? 'badge-success' : ''">
              {{ transaction.status === 'applied' ? t('historyActive') : transaction.status === 'rolled_back' ? t('historyUndone') : copy.pending }}
            </span>
          </div>
          <p class="history-game">{{ gameName(transaction.gameId) }}</p>
          <p class="history-meta"><span>{{ kindLabel(transaction.kind) }}</span><time :datetime="new Date(transaction.createdAt).toISOString()">{{ formatDate(transaction.createdAt) }}</time></p>
          <p v-if="transaction.status === 'applied' && blockedReason(transaction)" :id="`history-blocked-${transaction.id}`" class="history-blocked"><AppIcon name="info" :size="13" />{{ blockedReason(transaction) }}</p>
          <details class="disclosure history-files">
            <summary>{{ t('historyFiles') }} <span class="count">{{ transaction.files?.length ?? 0 }}</span></summary>
            <p class="path-text">{{ transaction.targetPath }}</p>
            <p v-for="file in transaction.files ?? []" :key="file.targetPath" class="path-text">{{ file.targetPath }}</p>
          </details>
        </div>
        <button
          v-if="transaction.status === 'applied'"
          class="btn btn-sm"
          :class="{ 'is-loading': busyId === transaction.id }"
          type="button"
          :disabled="working || Boolean(blockedReason(transaction))"
          :title="blockedReason(transaction)"
          :aria-label="t('ariaUndoNamed', { name: changeLabel(transaction) })"
          :aria-describedby="blockedReason(transaction) ? `history-blocked-${transaction.id}` : undefined"
          :aria-busy="busyId === transaction.id"
          @click="askUndo(transaction)"
        >
          <AppIcon v-if="busyId !== transaction.id" name="undo" :size="14" />
          {{ busyId === transaction.id ? t('historyUndoing') : t('historyUndo') }}
        </button>
      </article>
    </div>

    <ConfirmDialog
      v-if="pendingUndo"
      :title="t('historyUndoConfirmTitle')"
      :description="t('historyUndoConfirmDescription', { label: pendingUndo.label })"
      :confirm-label="t('historyUndo')"
      :cancel-label="t('cancel')"
      :details="[t('historyUndoConfirmDetailFiles', { count: pendingUndo.fileCount })]"
      @close="pendingUndo = null"
      @confirm="confirmUndo"
    />
  </section>
</template>

<style scoped>
.history-page { gap: var(--moddin-space-4); }
.history-page > * { flex-shrink: 0; }
.history-refresh { white-space: nowrap; }
.history-snapshots { border: 1px solid var(--moddin-line-soft); border-radius: var(--moddin-radius-lg); background: var(--moddin-surface-1); }
.history-snapshots > summary { display: flex; flex-wrap: wrap; align-items: center; gap: var(--moddin-space-2); padding: var(--moddin-space-3) var(--moddin-space-4); color: var(--moddin-text-soft); }
.history-snapshots > summary strong { font-size: var(--moddin-text-sm); }
.history-snapshots > summary > span { color: var(--moddin-text-muted); font-size: var(--moddin-text-xs); }
.history-snapshots :deep(.snapshot-panel) { border: 0; border-top: 1px solid var(--moddin-line-soft); border-radius: 0 0 var(--moddin-radius-lg) var(--moddin-radius-lg); }
.history-toolbar { display: flex; flex-wrap: wrap; align-items: center; gap: var(--moddin-space-3); }
.history-search { position: relative; flex: 1; min-width: 200px; }
.history-search > .app-icon { position: absolute; top: 50%; left: 12px; transform: translateY(-50%); color: var(--moddin-text-muted); pointer-events: none; }
.history-search > .input { padding-left: 36px; }
.history-results { display: flex; justify-content: space-between; gap: var(--moddin-space-3); color: var(--moddin-text-muted); font-size: var(--moddin-text-xs); }
.history-list { flex: 1; overflow-y: auto; padding: var(--moddin-space-2); }

.history-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: flex-start;
  gap: var(--moddin-space-3);
  border-radius: var(--moddin-radius-md);
  padding: var(--moddin-space-3);
}
.history-row + .history-row { border-top: 1px solid var(--moddin-line-soft); }
.history-row:hover { background: var(--moddin-surface-2); }

.history-marker {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  color: var(--moddin-success);
  background: var(--moddin-success-bg);
}
.history-row.undone .history-marker, .history-row.pending .history-marker { color: var(--moddin-text-muted); background: var(--moddin-neutral-bg); }

.history-copy { display: grid; gap: 2px; min-width: 0; }
.history-title { display: flex; flex-wrap: wrap; align-items: center; gap: var(--moddin-space-2); }
.history-title strong { font-size: var(--moddin-text-md); overflow-wrap: anywhere; }
.history-row.undone .history-title strong { color: var(--moddin-text-muted); }
.history-copy > p { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }
.history-copy > .history-game { color: var(--moddin-text-soft); }
.history-meta { display: flex; flex-wrap: wrap; gap: var(--moddin-space-2); }
.history-meta time::before { content: '·'; margin-right: var(--moddin-space-2); }
.history-copy > .history-blocked { display: flex; align-items: flex-start; gap: var(--moddin-space-1); margin-top: var(--moddin-space-1); color: var(--moddin-warning); }
.history-blocked .app-icon { margin-top: 2px; }

.history-files { margin-top: var(--moddin-space-1); }
.history-files > summary { color: var(--moddin-text-faint); font-size: var(--moddin-text-xs); }
.history-files > .path-text { margin-top: var(--moddin-space-1); }
@media (max-width: 1180px) {
  .history-search { flex-basis: 100%; }
}
@media (max-width: 960px) {
  .history-page { min-height: min-content; }
  .history-list { flex: none; overflow-y: visible; }
  .history-row { grid-template-columns: auto minmax(0, 1fr); }
  .history-row > .btn { grid-column: 2; justify-self: start; }
}
@media (max-width: 560px) {
  .history-page .page-header { align-items: flex-start; flex-direction: column; gap: var(--moddin-space-3); }
  .history-toolbar .segmented { flex-wrap: wrap; }
  .history-search { min-width: 0; }
  .history-results { flex-wrap: wrap; }
  .history-snapshots > summary > span { flex-basis: 100%; }
}
</style>
