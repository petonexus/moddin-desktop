<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import ConfirmDialog from '../../components/ui/ConfirmDialog.vue'
import type { TransactionRecord } from '../../types/transaction'

const props = defineProps<{
  transactions: TransactionRecord[]
  loading: boolean
  busyId: string | null
  gameName: (gameId: string) => string
  kindLabel: (kind: string) => string
  formatDate: (timestamp: number) => string
  blockedReason: (transaction: TransactionRecord) => string | undefined
}>()

const emit = defineEmits<{ refresh: []; undo: [transaction: TransactionRecord] }>()
const { t } = useI18n()

const activeOnly = ref(false)
const activeCount = computed(() => props.transactions.filter((item) => item.status === 'applied').length)
const visible = computed(() =>
  activeOnly.value ? props.transactions.filter((item) => item.status === 'applied') : props.transactions,
)

// Undo rewrites files in the game folder. Every row's button is
// labelled "Desfazer", so the list alone does not tell the user what
// they are about to revert — the dialog names it and counts the files.
const pendingUndo = ref<{ transaction: TransactionRecord; label: string; fileCount: number } | null>(null)

function askUndo(transaction: TransactionRecord) {
  pendingUndo.value = {
    transaction,
    label: transaction.label || transaction.kind,
    fileCount: transaction.files?.length ?? 0,
  }
}

function confirmUndo() {
  const pending = pendingUndo.value
  if (!pending) return
  pendingUndo.value = null
  emit('undo', pending.transaction)
}
</script>

<template>
  <section class="page">
    <header class="page-header">
      <div>
        <h1>{{ t('historyTitle') }}</h1>
        <p>{{ t('historySubtitle') }}</p>
      </div>
      <button class="btn" :class="{ 'is-loading': loading }" type="button" :disabled="loading" @click="emit('refresh')">
        <AppIcon v-if="!loading" name="refresh" :size="14" />
        {{ t('historyRefresh') }}
      </button>
    </header>

    <div class="history-toolbar">
      <div class="segmented" role="group" :aria-label="t('historyTitle')">
        <button type="button" :aria-pressed="!activeOnly" @click="activeOnly = false">
          {{ t('historyFilterAll') }} <span class="count">{{ transactions.length }}</span>
        </button>
        <button type="button" :aria-pressed="activeOnly" @click="activeOnly = true">
          {{ t('historyFilterActive') }} <span class="count">{{ activeCount }}</span>
        </button>
      </div>
    </div>

    <div class="panel history-list">
      <div v-if="loading && !transactions.length" class="empty-state">
        <span class="spinner" />
        <span>{{ t('historyLoading') }}</span>
      </div>
      <div v-else-if="!visible.length" class="empty-state">
        <AppIcon name="history" :size="28" />
        <strong>{{ t('historyEmptyTitle') }}</strong>
        <span>{{ t('historyEmptyHint') }}</span>
      </div>

      <article v-for="transaction in visible" :key="transaction.id" class="history-row" :class="{ undone: transaction.status !== 'applied' }">
        <div class="history-marker" aria-hidden="true">
          <AppIcon :name="transaction.status === 'applied' ? 'check' : 'undo'" :size="14" />
        </div>
        <div class="history-copy">
          <div class="history-title">
            <strong>{{ transaction.label }}</strong>
            <span class="badge" :class="transaction.status === 'applied' ? 'badge-success' : ''">
              {{ transaction.status === 'applied' ? t('historyActive') : t('historyUndone') }}
            </span>
          </div>
          <p>
            {{ gameName(transaction.gameId) }} · {{ kindLabel(transaction.kind) }} · {{ formatDate(transaction.createdAt) }}
          </p>
          <details class="disclosure history-files">
            <summary>{{ t('historyFiles') }}</summary>
            <p class="path-text">{{ transaction.targetPath }}</p>
            <p v-for="file in transaction.files ?? []" :key="file.targetPath" class="path-text">{{ file.targetPath }}</p>
          </details>
        </div>
        <button
          v-if="transaction.status === 'applied'"
          class="btn btn-sm"
          :class="{ 'is-loading': busyId === transaction.id }"
          type="button"
          :disabled="busyId === transaction.id || Boolean(blockedReason(transaction))"
          :title="blockedReason(transaction)"
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
.history-row.undone .history-marker { color: var(--moddin-text-faint); background: var(--moddin-neutral-bg); }

.history-copy { display: grid; gap: 2px; min-width: 0; }
.history-title { display: flex; flex-wrap: wrap; align-items: center; gap: var(--moddin-space-2); }
.history-title strong { font-size: var(--moddin-text-md); }
.history-row.undone .history-title strong { color: var(--moddin-text-muted); }
.history-copy > p { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }

.history-files { margin-top: var(--moddin-space-1); }
.history-files > summary { color: var(--moddin-text-faint); font-size: var(--moddin-text-xs); }
.history-files > .path-text { margin-top: var(--moddin-space-1); }
</style>
