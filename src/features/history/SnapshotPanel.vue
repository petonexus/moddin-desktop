<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import ConfirmDialog from '../../components/ui/ConfirmDialog.vue'
import ErrorCallout from '../../components/ui/ErrorCallout.vue'
import EmptyState from '../../components/ui/EmptyState.vue'
import { useFriendlyError, type FriendlyErrorRule } from '../../composables/useFriendlyError'
import type { TransactionRecord } from '../../types/transaction'
import type { Snapshot } from './types'
import { useSnapshots } from './useSnapshots'
import { historyCopyForLocale } from './copy'

/**
 * Point-in-time restore points for the History panel.
 *
 * A snapshot records which changes are active at the moment it is taken,
 * and restoring one undoes all of them newest-first. That is the most
 * destructive thing this panel can do — the transaction list's own "Undo"
 * reverts exactly one change and says so, while a restore reverts an
 * unbounded number and cannot be redone — so both destructive actions
 * here go through `ConfirmDialog` and name the count.
 *
 * The panel owns its own service calls rather than emitting for App.vue
 * to forward: `create_snapshot`, `list_snapshots`, `rollback_snapshot`
 * and `delete_snapshot` have no other caller, and a feature that emits
 * native command names upward puts them in a 115 KB coordinator.
 */

const props = defineProps<{
  transactions: TransactionRecord[]
  gameName: (gameId: string) => string
  formatDate: (timestamp: number) => string
  externalBusy?: boolean
}>()

/** Fired after a rollback, so the transaction list can be reloaded. */
const emit = defineEmits<{ changed: []; busy: [busy: boolean] }>()

const { t, locale } = useI18n()
const copy = computed(() => historyCopyForLocale(locale.value))

const {
  snapshots,
  loading,
  busyId,
  busyAction,
  error,
  errorContext,
  name,
  gameId,
  canCreate,
  refresh,
  create,
  rollback,
  remove,
} = useSnapshots()
const working = computed(() => loading.value || busyId.value !== null || Boolean(props.externalBusy))
watch(busyId, (id) => emit('busy', id !== null), { flush: 'sync' })

const errorRules = computed<FriendlyErrorRule[]>(() => [
  {
    context: 'load',
    match: /Could not read snapshot|Invalid Moddin snapshot id/i,
    title: t('historySnapshotErrorLoadTitle'),
    why: t('historySnapshotErrorLoadWhy'),
    showRaw: true,
  },
  {
    context: 'rollback',
    // `restore_record` is what fails mid-restore, and it fails per file.
    // Some changes are already undone at that point, so the copy has to
    // tell the user to look before pressing again.
    match: /Backup no longer exists|Could not restore|Could not recreate target directory|Cannot back up missing file|Close .* before undoing/i,
    title: t('historySnapshotErrorRollbackTitle'),
    why: t('historySnapshotErrorRollbackWhy'),
    showRaw: true,
  },
  {
    context: 'create',
    title: t('historySnapshotErrorCreateTitle'),
    why: t('historySnapshotErrorCreateWhy'),
    showRaw: true,
  },
  {
    context: 'delete',
    title: t('historySnapshotErrorDeleteTitle'),
    why: t('historySnapshotErrorDeleteWhy'),
    showRaw: true,
  },
  {
    title: t('historySnapshotErrorGenericTitle'),
    why: t('historySnapshotErrorGenericWhy'),
    showRaw: true,
  },
])

const friendlyError = useFriendlyError({ error, rules: () => errorRules.value, context: errorContext })

// A snapshot is per-game, and this panel is given no selected game — it
// only knows the games its transactions mention. So the game is a
// required explicit choice rather than a guess: preselecting one would
// quietly file a restore point under the wrong title, and "guess" is
// exactly how a rollback ends up reverting the wrong game's mods.
const gameChoices = computed(() => {
  const ids = new Set(
    props.transactions.filter((item) => item.status === 'applied').map((item) => item.gameId),
  )
  return [...ids].sort().map((id) => ({ gameId: id, label: props.gameName(id) }))
})

// Once a game's last active change is undone it stops being something
// you can snapshot, so keeping it selected would offer a create button
// for a game that is not in the list.
watch(gameChoices, (choices) => {
  if (gameId.value && !choices.some((choice) => choice.gameId === gameId.value)) {
    gameId.value = ''
  }
})

/** The transactions a snapshot captured that the loaded list can resolve. */
function capturedRecords(snapshot: Snapshot) {
  const byId = new Map(props.transactions.map((item) => [item.id, item]))
  return snapshot.transactionIds
    .map((id) => byId.get(id))
    .filter((item): item is TransactionRecord => Boolean(item))
}

/**
 * How many of the snapshot's changes are still active — the honest
 * number for the confirmation. Once a rollback has spent the snapshot
 * this is zero, and offering a Restore button that would return an
 * empty list and undo nothing is worse than saying it is spent.
 */
function remainingChanges(snapshot: Snapshot) {
  const captured = capturedRecords(snapshot)
  if (!captured.length) return snapshot.transactionIds.length
  return captured.filter((item) => item.status === 'applied').length
}

function affectedFiles(snapshot: Snapshot) {
  return capturedRecords(snapshot).reduce((total, item) => total + (item.files?.length ?? 0), 0)
}

const pendingRollback = ref<{ snapshot: Snapshot; changeCount: number; fileCount: number } | null>(null)
const pendingDelete = ref<{ snapshot: Snapshot; changeCount: number } | null>(null)

function askRollback(snapshot: Snapshot) {
  if (working.value) return
  pendingRollback.value = {
    snapshot,
    changeCount: remainingChanges(snapshot),
    fileCount: affectedFiles(snapshot),
  }
}

async function confirmRollback() {
  const pending = pendingRollback.value
  if (!pending || working.value) return
  await rollback(pending.snapshot.id)
  pendingRollback.value = null
  // Rollback runs one captured change at a time. A later failure can leave
  // earlier changes already undone, so both outcomes must reload History.
  emit('changed')
  await refresh({ preserveError: true })
}

function askDelete(snapshot: Snapshot) {
  if (working.value) return
  // The recorded count, not the live one: the line says what the
  // snapshot captured, which does not stop being true after a rollback.
  pendingDelete.value = { snapshot, changeCount: snapshot.transactionIds.length }
}

async function confirmDelete() {
  const pending = pendingDelete.value
  if (!pending || working.value) return
  await remove(pending.snapshot.id)
  pendingDelete.value = null
}

const rollbackDetails = computed(() => {
  const pending = pendingRollback.value
  if (!pending) return []
  return [
    t('historySnapshotRollbackDetailChanges', { count: pending.changeCount }),
    // Only offered when the list can name the files. A made-up count
    // would be worse than none.
    pending.fileCount > 0
      ? t('historySnapshotRollbackDetailFiles', { count: pending.fileCount })
      : null,
    t('historySnapshotRollbackDetailAfter'),
  ].filter((line): line is string => Boolean(line))
})

function submitCreate() {
  if (working.value) return
  void create()
}

onMounted(() => {
  void refresh()
})
</script>

<template>
  <section class="panel snapshot-panel" :aria-label="t('historySnapshotTitle')" :aria-busy="loading || busyId !== null">
    <header class="snapshot-header">
      <div>
        <h2>{{ t('historySnapshotTitle') }}</h2>
        <p>{{ copy.snapshotHint }}</p>
      </div>
      <button
        class="btn btn-sm"
        type="button"
        :class="{ 'is-loading': loading }"
        :disabled="working"
        :aria-label="copy.snapshotRefresh"
        :aria-busy="loading"
        @click="refresh()"
      >
        <AppIcon v-if="!loading" name="refresh" :size="14" />
        {{ t('historyRefresh') }}
      </button>
    </header>

    <form class="snapshot-create" @submit.prevent="submitCreate">
      <label class="field">
        <span>{{ t('historySnapshotGame') }}</span>
        <select v-model="gameId" class="select" required aria-required="true" :disabled="working || !gameChoices.length">
          <option value="">{{ t('historySnapshotGamePick') }}</option>
          <option v-for="choice in gameChoices" :key="choice.gameId" :value="choice.gameId">
            {{ choice.label }}
          </option>
        </select>
        <small v-if="!gameChoices.length">{{ t('historySnapshotNoGames') }}</small>
      </label>

      <label class="field">
        <span>{{ t('historySnapshotName') }}</span>
        <input
          v-model="name"
          class="input"
          type="text"
          required
          aria-required="true"
          :disabled="working || !gameChoices.length"
          :placeholder="t('historySnapshotNamePlaceholder')"
        />
      </label>

      <button
        class="btn btn-primary snapshot-create-button"
        type="submit"
        :class="{ 'is-loading': busyId === 'new' }"
        :disabled="!canCreate || working"
        :aria-busy="busyId === 'new'"
      >
        <AppIcon v-if="busyId !== 'new'" name="shield" :size="14" />
        {{ busyId === 'new' ? t('historySnapshotCreating') : t('historySnapshotCreate') }}
      </button>
    </form>

    <ErrorCallout :error="friendlyError" />

    <EmptyState
      v-if="loading && !snapshots.length"
      busy
      :description="t('historySnapshotLoading')"
    />
    <EmptyState
      v-else-if="!snapshots.length"
      icon="shield"
      :title="t('historySnapshotEmptyTitle')"
      :description="copy.snapshotEmptyHint"
    />

    <ul v-else class="snapshot-list">
      <li v-for="snapshot in snapshots" :key="snapshot.id" class="snapshot-row">
        <div class="snapshot-copy">
          <strong>{{ snapshot.name }}</strong>
          <p>{{ gameName(snapshot.gameId) }} · {{ formatDate(snapshot.createdAt) }}</p>
        </div>
        <span class="badge" :class="{ 'badge-success': remainingChanges(snapshot) > 0 }">
          {{ t('historySnapshotChanges', { count: remainingChanges(snapshot) }) }}
        </span>
        <div class="snapshot-actions">
          <button
            class="btn btn-sm"
            type="button"
            :class="{ 'is-loading': busyId === snapshot.id && busyAction === 'rollback' }"
            :disabled="working || remainingChanges(snapshot) === 0"
            :aria-busy="busyId === snapshot.id && busyAction === 'rollback'"
            :aria-label="copy.snapshotRestoreNamed.replace('{name}', snapshot.name)"
            :title="remainingChanges(snapshot) === 0 ? t('historySnapshotSpent') : undefined"
            @click="askRollback(snapshot)"
          >
            <AppIcon
              v-if="!(busyId === snapshot.id && busyAction === 'rollback')"
              name="undo"
              :size="14"
            />
            {{ busyId === snapshot.id && busyAction === 'rollback' ? t('historySnapshotRestoring') : t('historySnapshotRestore') }}
          </button>
          <button
            class="btn btn-sm btn-danger"
            type="button"
            :class="{ 'is-loading': busyId === snapshot.id && busyAction === 'delete' }"
            :disabled="working"
            :aria-busy="busyId === snapshot.id && busyAction === 'delete'"
            :aria-label="copy.snapshotDeleteNamed.replace('{name}', snapshot.name)"
            @click="askDelete(snapshot)"
          >
            <AppIcon v-if="!(busyId === snapshot.id && busyAction === 'delete')" name="close" :size="14" />
            {{ busyId === snapshot.id && busyAction === 'delete' ? t('historySnapshotDeleting') : t('historySnapshotDelete') }}
          </button>
        </div>
      </li>
    </ul>

    <ConfirmDialog
      v-if="pendingRollback"
      :title="copy.snapshotRollbackTitle"
      :description="t('historySnapshotRollbackDescription', { name: pendingRollback.snapshot.name })"
      :confirm-label="t('historySnapshotRollbackConfirm')"
      :cancel-label="t('cancel')"
      :details="rollbackDetails"
      :footnote="t('historySnapshotRollbackFootnote')"
      :busy="busyId !== null"
      @close="pendingRollback = null"
      @confirm="confirmRollback"
    />

    <ConfirmDialog
      v-if="pendingDelete"
      :title="t('historySnapshotDeleteTitle')"
      :description="t('historySnapshotDeleteDescription', { name: pendingDelete.snapshot.name })"
      :confirm-label="t('historySnapshotDeleteConfirm')"
      :cancel-label="t('cancel')"
      :details="[
        t('historySnapshotDeleteDetailKept', { count: pendingDelete.changeCount }),
        copy.snapshotDeleteDetailLost,
      ]"
      :busy="busyId !== null"
      @close="pendingDelete = null"
      @confirm="confirmDelete"
    />
  </section>
</template>

<style scoped>
.snapshot-panel { display: grid; gap: var(--moddin-space-3); padding: var(--moddin-space-3); max-height: 48vh; overflow-y: auto; }

.snapshot-header { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--moddin-space-3); }
.snapshot-header h2 { font-size: var(--moddin-text-md); }
.snapshot-header p { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }

.snapshot-create {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto;
  align-items: end;
  gap: var(--moddin-space-3);
}
.snapshot-create-button { white-space: nowrap; }

.snapshot-list { display: grid; }
.snapshot-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  align-items: center;
  gap: var(--moddin-space-3);
  padding: var(--moddin-space-2) 0;
}
.snapshot-row + .snapshot-row { border-top: 1px solid var(--moddin-line-soft); }

.snapshot-copy { min-width: 0; }
.snapshot-copy strong { display: block; font-size: var(--moddin-text-md); overflow-wrap: anywhere; }
.snapshot-copy p { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }

.snapshot-actions { display: flex; flex-wrap: wrap; gap: var(--moddin-space-2); }
@media (max-width: 1180px) {
  .snapshot-create { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  .snapshot-create-button { grid-column: 1 / -1; justify-self: start; }
  .snapshot-row { grid-template-columns: minmax(0, 1fr) auto; }
  .snapshot-actions { grid-column: 1 / -1; }
}
@media (max-width: 560px) {
  .snapshot-create { grid-template-columns: minmax(0, 1fr); }
  .snapshot-header { flex-wrap: wrap; }
  .snapshot-row { grid-template-columns: minmax(0, 1fr); }
  .snapshot-row > .badge { justify-self: start; }
}
</style>
