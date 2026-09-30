<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../ui/AppIcon.vue'
import BaseDialog from '../ui/BaseDialog.vue'
import ConfirmDialog from '../ui/ConfirmDialog.vue'
import ErrorCallout from '../ui/ErrorCallout.vue'
import { useCollectionInstall } from '../../composables/useCollectionInstall'
import { useFriendlyError, type FriendlyErrorRule } from '../../composables/useFriendlyError'
import type { CollectionStepStatus, CollectionSummary } from '../../types/collection'

/**
 * Preview and run one collection install.
 *
 * The dialog is the same wizard it always was — plan, confirm, step list,
 * and a stop-and-ask if a step fails — rebuilt on the current shell:
 * `BaseDialog` owns the overlay, Escape and focus, `ConfirmDialog` owns
 * the "this writes into your game folder" gate, and every failure goes
 * out through `useFriendlyError` + `ErrorCallout`.
 */
const props = defineProps<{
  summary: CollectionSummary
  gameId: string | null
  gameName: string | null
}>()

const emit = defineEmits<{ close: []; installed: [] }>()

const { t } = useI18n()
const install = useCollectionInstall()

/**
 * Installing a collection replaces files in a game folder, so it is
 * confirmed like any other destructive action — and while the
 * confirmation is up it is the *only* dialog mounted. Both dialogs
 * register a global Escape listener, and Escape has to dismiss the top
 * one, not the wizard behind it.
 */
const pendingConfirm = ref(false)

const plan = computed(() => props.summary.preset?.capabilities ?? [])

const error = computed<string | null>(() => {
  if (install.errorCode.value === 'noGame') return t('collectionNoGame')
  if (install.errorCode.value === 'noPreset') return t('collectionErrorNoPresetWhy')
  return install.error.value
})

const errorRules = computed<FriendlyErrorRule[]>(() => [
  { match: /denied|permission|0x80070005|access/i, title: t('collectionErrorInstallTitle'), why: t('collectionErrorInstallWhy'), showRaw: true },
  { match: /no space|ENOSPC|disk|quota/i, title: t('collectionErrorInstallTitle'), why: t('collectionErrorInstallWhy'), showRaw: true },
  { context: 'load', title: t('collectionErrorLoadTitle'), why: t('collectionErrorLoadWhy'), showRaw: true },
  { context: 'install', title: t('collectionErrorInstallTitle'), why: t('collectionErrorInstallWhy'), showRaw: true },
  { context: 'revert', title: t('collectionErrorInstallTitle'), why: t('collectionErrorInstallWhy'), showRaw: true },
  { title: t('collectionErrorGenericTitle'), why: t('collectionErrorGenericWhy'), showRaw: true },
])

/** Copy this panel already wrote; it needs surfacing, not re-titling. */
const alreadyLocalized = computed(() => [t('collectionNoGame'), t('collectionErrorNoPresetWhy')])

const friendlyError = useFriendlyError({
  error,
  rules: () => errorRules.value,
  context: install.errorContext,
  verbatim: alreadyLocalized,
})

const confirmDetails = computed(() => [
  t('collectionInstallConfirmDescription', {
    count: plan.value.length,
    game: props.gameName ?? t('collectionInstallConfirmGenericGame'),
  }),
  t('collectionInstallConfirmDetailReplace'),
  t('collectionInstallConfirmDetailBackup'),
  t('collectionInstallConfirmDetailGameClosed'),
])

const stepLabel: Record<CollectionStepStatus, string> = {
  pending: 'collectionStepPending',
  running: 'collectionStepRunning',
  completed: 'collectionStepCompleted',
  failed: 'collectionStepFailed',
  reverted: 'collectionStepReverted',
}

const stepIcon: Record<CollectionStepStatus, 'info' | 'refresh' | 'check' | 'alert' | 'undo'> = {
  pending: 'info',
  running: 'refresh',
  completed: 'check',
  failed: 'alert',
  reverted: 'undo',
}

/** The heading the current phase is under, below the collection's name. */
const phaseHeading = computed(() => {
  switch (install.phase.value) {
    case 'paused':
      return t('collectionPausedHeading')
    case 'completed':
      return t('collectionCompletedHeading')
    case 'reverted':
      return t('collectionAbortedHeading')
    default:
      return ''
  }
})

const dialogTitle = computed(() => props.summary.displayName || props.summary.id)

const dialogDescription = computed(() => {
  switch (install.phase.value) {
    case 'idle':
      return t('collectionSubtitle')
    case 'installing':
      return t('collectionRunning')
    case 'paused':
      return t('collectionPausedBody', { capability: install.session.value?.failedCapability ?? '' })
    case 'completed':
      return t('collectionCompletedBody')
    case 'reverting':
      return t('collectionReverting')
    case 'reverted':
      return t('collectionAbortedBody')
    default:
      return t('collectionErrorHeading')
  }
})

const canInstall = computed(() => plan.value.length > 0 && props.gameId !== null)
const busy = computed(() => install.phase.value === 'installing' || install.phase.value === 'reverting')

async function confirmInstall() {
  pendingConfirm.value = false
  await install.start(props.summary, props.gameId)
  if (install.phase.value === 'completed') emit('installed')
}

function close() {
  emit('close')
}

watch(
  () => props.summary.id,
  () => install.reset(),
)
</script>

<template>
  <ConfirmDialog
    v-if="pendingConfirm"
    :title="t('collectionInstallConfirmTitle')"
    :description="t('collectionInstallConfirmDescription', { count: plan.length, game: gameName ?? t('collectionInstallConfirmGenericGame') })"
    :confirm-label="t('collectionInstall')"
    :details="confirmDetails"
    @close="pendingConfirm = false"
    @confirm="confirmInstall"
  />

  <BaseDialog
    v-else
    :title="dialogTitle"
    :eyebrow="t('collection')"
    :description="dialogDescription"
    size="lg"
    @close="close"
  >
    <div class="collection-run">
      <ErrorCallout :error="friendlyError" />

      <p v-if="phaseHeading" class="overline">{{ phaseHeading }}</p>

      <div v-if="install.phase.value === 'idle'" class="collection-run-plan">
        <p class="overline">{{ t('collectionWillInstall', { count: plan.length }) }}</p>
        <ol class="collection-plan-list">
          <li v-for="capability in plan" :key="capability.id">
            <strong>{{ capability.displayName || capability.id }}</strong>
            <span v-if="capability.rationale">{{ capability.rationale }}</span>
          </li>
        </ol>
      </div>

      <div v-else-if="install.session.value" class="collection-run-steps">
        <p v-if="busy" class="collection-progress">
          <span class="progress-bar"><span :style="{ width: `${install.progress.value}%` }" /></span>
          <span>{{ install.progress.value }}%</span>
        </p>
        <ol class="collection-step-list">
          <li v-for="step in install.session.value.steps" :key="step.capabilityId" :class="`is-${step.status}`">
            <AppIcon :name="stepIcon[step.status]" :size="14" />
            <span class="collection-step-name">{{ step.displayName }}</span>
            <span class="collection-step-status">{{ t(stepLabel[step.status]) }}</span>
          </li>
        </ol>
      </div>

      <p v-else class="collection-run-idle">{{ t('collectionErrorNoPresetWhy') }}</p>
    </div>

    <template #footer>
      <button
        v-if="install.phase.value === 'idle'"
        class="btn"
        type="button"
        :disabled="busy"
        @click="close"
      >
        {{ t('actionCancel') }}
      </button>
      <button
        v-if="install.phase.value === 'idle'"
        class="btn btn-primary"
        type="button"
        :disabled="!canInstall"
        :title="canInstall ? undefined : t('collectionNoGame')"
        @click="pendingConfirm = true"
      >
        <AppIcon name="arrow-up" :size="16" />
        {{ t('collectionInstall') }}
      </button>

      <template v-else-if="install.phase.value === 'paused'">
        <button class="btn" type="button" :disabled="busy" @click="install.decide('abort')">
          <AppIcon name="undo" :size="16" />
          {{ t('collectionPauseAbort') }}
        </button>
        <button class="btn btn-primary" type="button" :disabled="busy" @click="install.decide('continue')">
          {{ t('collectionPauseContinue') }}
        </button>
      </template>

      <button v-else class="btn btn-primary" type="button" :disabled="busy" @click="close">
        {{ t('close') }}
      </button>
    </template>
  </BaseDialog>
</template>

<style scoped>
.collection-run { display: grid; gap: var(--moddin-space-3); }

.collection-run-plan,
.collection-run-steps { display: grid; gap: var(--moddin-space-2); }

.collection-plan-list,
.collection-step-list { display: grid; gap: var(--moddin-space-2); margin: 0; padding: 0; list-style: none; }

.collection-plan-list li { display: grid; gap: 2px; }
.collection-plan-list span { color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }

.collection-step-list li {
  display: flex;
  align-items: center;
  gap: var(--moddin-space-2);
  color: var(--moddin-text-soft);
  font-size: var(--moddin-text-sm);
}
.collection-step-list li.is-completed { color: var(--moddin-text); }
.collection-step-list li.is-failed { color: var(--moddin-danger); }

.collection-step-name { flex: 1 1 auto; }
.collection-step-status { color: var(--moddin-text-muted); }

.collection-progress { display: flex; align-items: center; gap: var(--moddin-space-3); margin: 0; font-size: var(--moddin-text-sm); }
.progress-bar { flex: 1 1 auto; height: 4px; border-radius: 2px; background: var(--moddin-line-soft); overflow: hidden; }
.progress-bar > span { display: block; height: 100%; background: var(--moddin-accent); }

.collection-run-idle { margin: 0; color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }
</style>
