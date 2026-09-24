<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import type { ModuleVerification } from '../../types/module-verification'

export type ModuleCardState = 'active' | 'available' | 'attention' | 'unknown' | 'checking' | 'planned'

export interface ModuleCardUpdate {
  hasSource: boolean
  available: boolean
  summary: string
  releaseUrl: string | null
  busy: boolean
}

const props = defineProps<{
  name: string
  description: string
  state: ModuleCardState
  tag?: { label: string; tone: 'success' | 'info' | 'warning' | 'danger' | 'neutral' } | null
  actionLabel: string
  actionPrimary: boolean
  actionBusy: boolean
  actionDisabled: boolean
  blockedReason?: string
  verification?: ModuleVerification
  checkedAtLabel?: string
  verifyBusy: boolean
  removeLabel?: string | null
  update?: ModuleCardUpdate | null
}>()

const emit = defineEmits<{
  action: []
  verify: []
  remove: []
  'check-update': []
  'open-release': []
}>()

const { t } = useI18n()

const stateMeta = computed(() => {
  switch (props.state) {
    case 'active': return { label: t('stateActive'), tone: 'badge-success' }
    case 'available': return { label: t('stateAvailable'), tone: 'badge-info' }
    case 'attention': return { label: t('stateAttention'), tone: 'badge-warning' }
    case 'checking': return { label: t('stateChecking'), tone: 'is-loading' }
    case 'planned': return { label: t('statePlanned'), tone: 'badge-plain' }
    default: return { label: t('stateUnknown'), tone: '' }
  }
})

const failedChecks = computed(() => props.verification?.checks.filter((item) => !item.passed) ?? [])
const passedCount = computed(() => (props.verification?.checks.length ?? 0) - failedChecks.value.length)
const isPlanned = computed(() => props.state === 'planned')
</script>

<template>
  <article class="module-card" :class="[`is-${state}`]">
    <header class="module-card-header">
      <h4>{{ name }}</h4>
      <span class="badge" :class="stateMeta.tone">{{ stateMeta.label }}</span>
    </header>

    <p class="module-card-description">{{ description }}</p>

    <div v-if="tag || update?.available" class="module-card-tags">
      <span v-if="tag" class="badge badge-plain" :class="`badge-${tag.tone}`">{{ tag.label }}</span>
      <button v-if="update?.available" type="button" class="update-pill" @click="emit('open-release')">
        <AppIcon name="arrow-up" :size="12" />
        {{ update.summary }}
      </button>
    </div>

    <div v-if="state === 'attention' && failedChecks.length" class="module-card-issues">
      <AppIcon name="alert" :size="14" />
      <div>
        <strong>{{ t('cardIssuesTitle') }}</strong>
        <ul>
          <li v-for="item in failedChecks.slice(0, 3)" :key="item.label">{{ item.label }}</li>
          <li v-if="failedChecks.length > 3" class="more">{{ t('cardIssuesMore', { count: failedChecks.length - 3 }) }}</li>
        </ul>
      </div>
    </div>

    <footer v-if="!isPlanned" class="module-card-actions">
      <button
        class="btn btn-sm"
        :class="{ 'btn-primary': actionPrimary, 'is-loading': actionBusy }"
        type="button"
        :disabled="actionDisabled"
        :title="blockedReason"
        @click="emit('action')"
      >
        {{ actionLabel }}
      </button>
      <button
        class="btn btn-ghost btn-sm"
        :class="{ 'is-loading': verifyBusy }"
        type="button"
        :disabled="verifyBusy || Boolean(blockedReason)"
        :title="blockedReason"
        @click="emit('verify')"
      >
        {{ verifyBusy ? t('actionChecking') : t('actionCheck') }}
      </button>
      <button
        v-if="removeLabel"
        class="btn btn-danger btn-sm push-right"
        type="button"
        :disabled="actionDisabled"
        :title="blockedReason"
        @click="emit('remove')"
      >
        {{ removeLabel }}
      </button>
    </footer>

    <details v-if="!isPlanned && (verification || update || $slots.details)" class="disclosure module-card-details">
      <summary>
        {{ t('cardDetails') }}
        <small v-if="verification">· {{ t('checklistSummary', { passed: passedCount, total: verification.checks.length }) }}</small>
      </summary>

      <div class="module-card-details-body">
        <slot name="details" />

        <section v-if="verification" class="detail-block">
          <div class="detail-heading">
            <h5>{{ t('checklistTitle') }}</h5>
            <small v-if="checkedAtLabel">{{ checkedAtLabel }}</small>
          </div>
          <ul class="checklist">
            <li v-for="item in verification.checks" :key="item.label" :class="item.passed ? 'ok' : 'fail'">
              <AppIcon :name="item.passed ? 'check' : 'alert'" :size="13" />
              <span>
                {{ item.label }}
                <small v-if="item.detail">{{ item.detail }}</small>
              </span>
            </li>
          </ul>
        </section>

        <section v-if="update" class="detail-block">
          <div class="detail-heading">
            <h5>{{ t('updateSectionTitle') }}</h5>
          </div>
          <p class="detail-text">{{ update.summary }}</p>
          <div v-if="update.hasSource" class="detail-actions">
            <button
              class="btn btn-sm"
              :class="{ 'is-loading': update.busy }"
              type="button"
              :disabled="update.busy"
              @click="emit('check-update')"
            >
              {{ update.busy ? t('updateChecking') : t('updateCheck') }}
            </button>
            <button v-if="update.releaseUrl" class="btn btn-ghost btn-sm" type="button" @click="emit('open-release')">
              <AppIcon name="external" :size="13" />
              {{ t('updateOpenRelease') }}
            </button>
          </div>
        </section>
      </div>
    </details>
  </article>
</template>

<style scoped>
.module-card {
  display: flex;
  flex-direction: column;
  gap: var(--moddin-space-3);
  border: 1px solid var(--moddin-line);
  border-radius: var(--moddin-radius-lg);
  padding: var(--moddin-space-4);
  background: var(--moddin-surface-2);
  transition: border-color var(--moddin-normal) var(--moddin-ease);
}
.module-card:hover { border-color: var(--moddin-line-strong); }
.module-card.is-active { border-color: var(--moddin-success-line); }
.module-card.is-attention { border-color: var(--moddin-warning-line); }
.module-card.is-planned { opacity: 0.7; }

.module-card-header { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--moddin-space-3); }
.module-card-header h4 { font-size: var(--moddin-text-base); }

.module-card-description { color: var(--moddin-text-muted); font-size: var(--moddin-text-md); line-height: 1.5; }

.module-card-tags { display: flex; flex-wrap: wrap; gap: var(--moddin-space-2); }

.update-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  border: 1px solid var(--moddin-accent-ring);
  border-radius: var(--moddin-radius-pill);
  padding: 2px 9px;
  color: var(--moddin-accent-text);
  background: var(--moddin-accent-soft);
  font-size: var(--moddin-text-xs);
  font-weight: 650;
  cursor: pointer;
}

.module-card-issues {
  display: flex;
  gap: var(--moddin-space-2);
  border-radius: var(--moddin-radius-md);
  padding: var(--moddin-space-2) var(--moddin-space-3);
  color: var(--moddin-warning);
  background: var(--moddin-warning-bg);
  font-size: var(--moddin-text-sm);
}
.module-card-issues .app-icon { margin-top: 2px; }
.module-card-issues strong { display: block; margin-bottom: 2px; }
.module-card-issues ul { display: grid; gap: 2px; margin: 0; padding: 0; list-style: none; color: var(--moddin-text-soft); }
.module-card-issues li::before { content: '✕ '; color: var(--moddin-warning); }
.module-card-issues li.more::before { content: ''; }

.module-card-actions { display: flex; flex-wrap: wrap; align-items: center; gap: var(--moddin-space-2); margin-top: auto; }
.push-right { margin-left: auto; }

.module-card-details { border-top: 1px solid var(--moddin-line-soft); padding-top: var(--moddin-space-3); }
.module-card-details > summary { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); font-weight: 600; }
.module-card-details > summary:hover { color: var(--moddin-text); }
.module-card-details > summary small { color: var(--moddin-text-faint); font-weight: 500; }
.module-card-details-body { display: grid; gap: var(--moddin-space-4); margin-top: var(--moddin-space-3); }

.detail-block { display: grid; gap: var(--moddin-space-2); }
.detail-heading { display: flex; align-items: baseline; justify-content: space-between; gap: var(--moddin-space-2); }
.detail-heading h5 { margin: 0; color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }
.detail-heading small { color: var(--moddin-text-faint); font-size: var(--moddin-text-xs); }
.detail-text { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }
.detail-actions { display: flex; flex-wrap: wrap; gap: var(--moddin-space-2); }

.checklist { display: grid; gap: 6px; margin: 0; padding: 0; list-style: none; }
.checklist li { display: flex; align-items: flex-start; gap: var(--moddin-space-2); color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }
.checklist li .app-icon { margin-top: 2px; }
.checklist li.ok .app-icon { color: var(--moddin-success); }
.checklist li.fail .app-icon { color: var(--moddin-warning); }
.checklist small { display: block; color: var(--moddin-text-faint); font-size: var(--moddin-text-xs); }
</style>
