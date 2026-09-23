<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { dateLocaleFor } from '../../i18n/locale'
import type { TransactionRecord } from '../../types/transaction'

const props = defineProps<{
  transactions: TransactionRecord[]
  loading: boolean
  rollbackBusyId: string | null
  blockRollbackForRunningGame: boolean
}>()

const emit = defineEmits<{
  refresh: []
  rollback: [transactionId: string]
}>()

const { t, locale } = useI18n()

function statusLabel(status: string): string {
  return t(`transactionStatus_${status}`)
}

function formatDate(timestamp: number) {
  return new Intl.DateTimeFormat(dateLocaleFor(locale.value), {
    dateStyle: 'short',
    timeStyle: 'medium',
  }).format(new Date(timestamp))
}
</script>

<template>
  <section class="transactions-view">
    <header class="topbar transactions-topbar">
      <div>
        <p class="eyebrow">{{ t('rollbackHistory') }}</p>
        <h1>{{ t('transactions') }}</h1>
        <p class="subtle">{{ t('everyDestructive') }}</p>
      </div>
      <button
        class="secondary-button"
        :class="{ 'is-loading': loading }"
        :disabled="loading"
        @click="emit('refresh')"
      >
        {{ loading ? t('refreshing') : t('refresh') }}
      </button>
    </header>

    <section class="transactions-panel">
      <div v-if="loading && transactions.length === 0" class="empty-state">
        <span class="loading-state" role="status" aria-live="polite">
          <span class="loading-spinner" aria-hidden="true"></span>
          {{ t('loadingTransactions') }}
        </span>
      </div>
      <div v-else-if="transactions.length === 0" class="empty-state">
        {{ t('noTransactions') }}
      </div>

      <article
        v-for="transaction in transactions"
        :key="transaction.id"
        class="transaction-row"
      >
        <div class="transaction-state" :class="transaction.status"></div>
        <div class="transaction-copy">
          <div class="transaction-title-row">
            <strong>{{ transaction.label }}</strong>
            <span
              class="status"
              :class="transaction.status === 'applied' ? 'supported' : 'unsupported'"
            >
              {{ statusLabel(transaction.status) }}
            </span>
          </div>
          <span>
            {{ formatDate(transaction.createdAt) }} · {{ transaction.kind }} · {{ transaction.gameId }}
          </span>
          <code>{{ transaction.targetPath }}</code>
        </div>
        <button
          class="secondary-button"
          :class="{ 'is-loading': rollbackBusyId === transaction.id }"
          :disabled="transaction.status !== 'applied'
            || rollbackBusyId === transaction.id
            || blockRollbackForRunningGame"
          :title="blockRollbackForRunningGame ? t('gameRunningActionBlocked') : undefined"
          @click="emit('rollback', transaction.id)"
        >
          {{ rollbackBusyId === transaction.id ? t('restoring') : t('undo') }}
        </button>
      </article>
    </section>
  </section>
</template>
