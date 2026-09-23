<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDialogLifecycle } from '../../composables/useDialogLifecycle'
import { useCollectionInstall } from '../../composables/useCollectionInstall'
import type { CollectionSpec } from '../../types/collection'

const props = defineProps<{
  open: boolean
  spec: CollectionSpec | null
  gameContext: { gameId: string; gameName: string; installDir: string; executableDir: string }
}>()

const emit = defineEmits<{
  close: []
}>()

const { t } = useI18n()
const install = useCollectionInstall()
const { closeDialog } = useDialogLifecycle(computed({
  get: () => props.open,
  set: (v: boolean) => {
    if (!v) emit('close')
  },
}))

function close() {
  emit('close')
}

function closeOnBackdrop(event: MouseEvent) {
  if (event.target === event.currentTarget) {
    close()
  }
}

function statusLabel(status: string): string {
  switch (status) {
    case 'pending':
      return t('collectionStepPending')
    case 'running':
      return t('collectionStepRunning')
    case 'completed':
      return t('collectionStepCompleted')
    case 'skipped':
      return t('collectionStepSkipped')
    case 'failed':
      return t('collectionStepFailed')
    default:
      return status
  }
}

function statusIcon(status: string): string {
  switch (status) {
    case 'completed':
      return '✓'
    case 'failed':
      return '✕'
    case 'skipped':
      return '⤼'
    case 'running':
      return '•'
    default:
      return '○'
  }
}

async function start() {
  if (!props.spec) return
  await install.start(props.spec.id, props.gameContext)
}

async function decide(decision: 'continue' | 'abort') {
  await install.decide(decision)
}

async function abort() {
  await install.abort()
}
</script>

<template>
  <div v-if="open" class="col-backdrop" @click="closeOnBackdrop">
    <section class="col-panel" role="dialog" aria-modal="true" tabindex="-1">
      <header class="col-header">
        <div>
          <small>COLLECTION</small>
          <h2>{{ spec?.displayName ?? t('collectionInstallHeading') }}</h2>
          <p v-if="spec?.description">{{ spec.description }}</p>
        </div>
        <button
          class="col-icon-button"
          type="button"
          :aria-label="t('collectionClose')"
          @click="close"
        >
          ×
        </button>
      </header>

      <div v-if="install.lastError.value" class="col-error" role="alert">
        {{ install.lastError.value }}
      </div>

      <!-- Step list — visible from start until finished. -->
      <div v-if="install.session.value" class="col-progress">
        <div class="col-progress-bar" role="progressbar" :aria-valuenow="install.progress.value">
          <div class="col-progress-fill" :style="{ width: `${install.progress.value}%` }" />
        </div>
        <ol class="col-step-list">
          <li
            v-for="(step, index) in install.session.value.steps"
            :key="step.capabilityId"
            :class="['col-step', `col-step-${step.status}`]"
          >
            <span class="col-step-icon" aria-hidden="true">{{ statusIcon(step.status) }}</span>
            <span class="col-step-name">{{ step.capabilityId }}</span>
            <span class="col-step-status">{{ statusLabel(step.status) }}</span>
            <small v-if="step.error" class="col-step-error">{{ step.error }}</small>
            <small
              v-if="!step.error && index === install.session.value.nextIndex && step.status === 'running'"
              class="col-step-running"
            >
              <span class="loading-spinner" aria-hidden="true"></span>
            </small>
          </li>
        </ol>
      </div>

      <!-- Phase: idle — show what will run and a Start button. -->
      <div v-if="install.phase.value === 'idle' && spec" class="col-phase">
        <p class="col-summary">
          {{ t('collectionWillInstall', { count: spec.capabilities.length }) }}
        </p>
        <div class="col-actions">
          <button class="col-primary" type="button" @click="start">
            {{ t('collectionInstall') }}
          </button>
        </div>
      </div>

      <!-- Phase: starting / running — show a waiting hint, no buttons. -->
      <div v-if="install.phase.value === 'starting' || install.phase.value === 'continuing' || install.phase.value === 'aborted'" class="col-phase">
        <p class="col-running-hint">
          <span class="loading-spinner" aria-hidden="true"></span>
          {{ t('collectionRunning') }}
        </p>
      </div>

      <!-- Phase: paused — the "ask mid-way" prompt. -->
      <div v-if="install.isPaused.value" class="col-phase col-pause-prompt">
        <strong>{{ t('collectionPausedHeading') }}</strong>
        <p>
          {{ t('collectionPausedBody', { capability: install.session.value?.failedCapability ?? '' }) }}
        </p>
        <p class="col-pause-error">{{ install.session.value?.lastError }}</p>
        <div class="col-actions">
          <button class="col-secondary" type="button" @click="decide('abort')">
            {{ t('collectionPauseAbort') }}
          </button>
          <button class="col-primary" type="button" @click="decide('continue')">
            {{ t('collectionPauseContinue') }}
          </button>
        </div>
      </div>

      <!-- Phase: completed. -->
      <div v-if="install.phase.value === 'completed'" class="col-phase col-success">
        <strong>{{ t('collectionCompletedHeading') }}</strong>
        <p>{{ t('collectionCompletedBody') }}</p>
        <div class="col-actions">
          <button class="col-primary" type="button" @click="close">
            {{ t('collectionDone') }}
          </button>
        </div>
      </div>

      <!-- Phase: aborted. -->
      <div v-if="install.phase.value === 'aborted' && install.rollbackReport.value" class="col-phase col-success">
        <strong>{{ t('collectionAbortedHeading') }}</strong>
        <p>{{ t('collectionAbortedBody') }}</p>
        <div class="col-actions">
          <button class="col-primary" type="button" @click="close">
            {{ t('collectionDone') }}
          </button>
        </div>
      </div>

      <!-- Phase: error. -->
      <div v-if="install.phase.value === 'error'" class="col-phase col-error">
        <strong>{{ t('collectionErrorHeading') }}</strong>
        <div class="col-actions">
          <button class="col-secondary" type="button" @click="close">
            {{ t('collectionDone') }}
          </button>
          <button v-if="install.session.value" class="col-primary" type="button" @click="abort">
            {{ t('collectionRollback') }}
          </button>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.col-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  z-index: 9200;
  display: flex;
  align-items: center;
  justify-content: center;
}

.col-panel {
  width: min(640px, 96vw);
  max-height: 88vh;
  overflow: auto;
  background: var(--moddin-surface, #141823);
  color: var(--moddin-text, #e8ecf2);
  border-radius: 12px;
  padding: 1.25rem 1.5rem 1.5rem;
  box-shadow: 0 24px 48px rgba(0, 0, 0, 0.6);
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.col-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
}

.col-header small {
  letter-spacing: 0.08em;
  color: var(--moddin-accent, #7aa2f7);
  font-size: 0.75rem;
  font-weight: 700;
}

.col-header h2 {
  margin: 0.1rem 0 0.2rem;
  font-size: 1.05rem;
}

.col-header p {
  margin: 0;
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.85rem;
}

.col-icon-button {
  background: transparent;
  border: 1px solid var(--moddin-border, #3a4252);
  color: inherit;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  cursor: pointer;
  font-size: 1.1rem;
}

.col-error {
  background: rgba(255, 90, 90, 0.12);
  border: 1px solid rgba(255, 90, 90, 0.4);
  color: #ff8c8c;
  padding: 0.5rem 0.7rem;
  border-radius: 6px;
  font-size: 0.85rem;
}

.col-success {
  background: rgba(40, 200, 120, 0.12);
  border: 1px solid rgba(40, 200, 120, 0.4);
  color: #2ecf86;
  padding: 0.7rem 0.9rem;
  border-radius: 6px;
  font-size: 0.9rem;
}

.col-progress {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.col-progress-bar {
  width: 100%;
  height: 6px;
  border-radius: 3px;
  background: var(--moddin-surface-2, #1a1f29);
  overflow: hidden;
}

.col-progress-fill {
  height: 100%;
  background: var(--moddin-accent, #7aa2f7);
  transition: width 200ms ease;
}

.col-step-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.col-step {
  display: grid;
  grid-template-columns: 24px 1fr auto;
  grid-column-gap: 0.5rem;
  align-items: center;
  background: var(--moddin-surface-2, #1a1f29);
  border: 1px solid var(--moddin-border, #3a4252);
  border-radius: 6px;
  padding: 0.4rem 0.6rem;
  font-size: 0.85rem;
}

.col-step-icon {
  text-align: center;
  font-weight: 600;
}

.col-step-name {
  font-family: 'Cascadia Code', 'Consolas', monospace;
}

.col-step-status {
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.75rem;
}

.col-step-completed .col-step-icon {
  color: #2ecf86;
}

.col-step-failed .col-step-icon {
  color: #ff8c8c;
}

.col-step-skipped .col-step-icon {
  color: var(--moddin-warning, #f0b432);
}

.col-step-running .col-step-icon {
  color: var(--moddin-accent, #7aa2f7);
}

.col-step-error {
  grid-column: 2 / span 2;
  color: #ff8c8c;
  font-size: 0.75rem;
}

.col-step-running {
  grid-column: 3 / span 1;
  display: flex;
  justify-content: flex-end;
}

.col-phase {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.col-summary {
  margin: 0;
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.85rem;
}

.col-pause-prompt {
  background: rgba(240, 180, 50, 0.08);
  border: 1px solid rgba(240, 180, 50, 0.4);
  border-radius: 6px;
  padding: 0.7rem 0.9rem;
  color: #f0b432;
}

.col-pause-error {
  margin: 0.3rem 0 0;
  font-family: 'Cascadia Code', 'Consolas', monospace;
  font-size: 0.78rem;
  color: #ff8c8c;
}

.col-running-hint {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.85rem;
}

.col-actions {
  display: flex;
  gap: 0.5rem;
  justify-content: flex-end;
  margin-top: 0.4rem;
}

.col-primary {
  background: var(--moddin-accent, #7aa2f7);
  color: #0c0e15;
  border: none;
  border-radius: 6px;
  padding: 0.45rem 1rem;
  font-weight: 600;
  cursor: pointer;
}

.col-secondary {
  background: transparent;
  border: 1px solid var(--moddin-accent, #7aa2f7);
  color: var(--moddin-accent, #7aa2f7);
  border-radius: 6px;
  padding: 0.4rem 0.9rem;
  cursor: pointer;
}
</style>
