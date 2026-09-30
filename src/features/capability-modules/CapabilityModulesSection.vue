<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import ConfirmDialog from '../../components/ui/ConfirmDialog.vue'
import EmptyState from '../../components/ui/EmptyState.vue'
import ModuleCard, { type ModuleCardState, type ModuleCardVerification } from '../library/ModuleCard.vue'
import type { TransactionRecord } from '../../types/transaction'
import type { CapabilityOrigin, CapabilitySummary } from '../../types/capability'
import type { ModuleVerificationCheck } from '../../types/module-verification'
import { useBackendText } from '../../composables/useBackendText'
import { useCapabilityModules } from './useCapabilityModules'
import type {
  CapabilityCardState,
  CapabilityConfigValue,
  CapabilitySummaryView,
} from './types'

const props = defineProps<{
  gameId: string | null
  gameName: string | null
  installDir: string | null
  executableDir: string | null
  engine: string | null
  excludeIds: string[]
  transactions: TransactionRecord[]
}>()

const emit = defineEmits<{
  'refresh-request': []
}>()

const { t } = useI18n()
const { keyed } = useBackendText()

const modules = useCapabilityModules({
  gameId: () => props.gameId,
  gameName: () => props.gameName,
  installDir: () => props.installDir,
  executableDir: () => props.executableDir,
  engine: () => props.engine,
  excludeIds: () => props.excludeIds,
  transactions: () => props.transactions,
  onChanged: () => emit('refresh-request'),
})

const {
  visibleCapabilities,
  loading,
  loadError,
  ensureLoaded,
  stateFor,
  isInstalled,
  missingRequiredFields,
  compatibilityBlocks,
  install,
  uninstall,
  verify,
} = modules

const dirsMissing = computed(() => !props.installDir || !props.executableDir)
const blockedReasonText = computed(() => (dirsMissing.value ? t('capabilityNoDirs') : undefined))

function schemaOf(capabilityId: string) {
  return stateFor(capabilityId).spec?.configSchema ?? []
}

function safetyNotesOf(capabilityId: string) {
  return stateFor(capabilityId).spec?.safetyNotes ?? []
}

/** Failing `compatibility` probe, if any — the only case that blocks Apply. */
function blockedCompatibility(state: CapabilityCardState) {
  return state.compatibility && !state.compatibility.passed ? state.compatibility : null
}

/**
 * UX-21: the card's description and every check label arrive from Rust
 * with the locale key the service layer resolved, or with nothing at
 * all when the backend string is not a declared id — in which case the
 * recipe's own sentence is what the user reads. `keyed` is that second
 * case made explicit in one place, so neither `ModuleCard` nor this
 * template has to know which strings crossed the boundary.
 */
function descriptionOf(capability: CapabilitySummaryView): string {
  return keyed(capability.descriptionKey, capability.description || capability.id)
}

/**
 * The capability runner reports `detail: null` for checks that carry no
 * detail; the card's check shape has no null. Everything else (including
 * the backend's own summary sentence) is left to the card to render.
 */
function cardChecks(state: CapabilityCardState): ModuleVerificationCheck[] {
  return (state.verification?.checks ?? []).map((item) => ({
    label: keyed(item.labelKey, item.label),
    passed: item.passed,
    detail: item.detail ?? undefined,
  }))
}

function cardVerification(state: CapabilityCardState): ModuleCardVerification | null {
  return state.verification ? { checks: cardChecks(state) } : null
}

/**
 * Same derivation the catalog cards use in `App.vue`: a check that failed
 * must not read as a healthy mod. The runner never reports `installed`, so
 * the transaction history owns the installed half of the state.
 */
function cardState(capabilityId: string): ModuleCardState {
  const state = stateFor(capabilityId)
  if (state.verifyBusy) return 'checking'
  if (state.verification && cardChecks(state).some((check) => !check.passed)) return 'attention'
  return isInstalled(capabilityId) ? 'active' : 'available'
}

/** Ids auto-installed by the backend as dependencies of the last install. */
function installedDependencyNames(capabilityId: string): string {
  return stateFor(capabilityId).installedDependencies.join(', ')
}

function configValue(state: CapabilityCardState, name: string): CapabilityConfigValue {
  return state.configValues[name] ?? ''
}

/**
 * UX-28: the field's real validation state.
 *
 * `missingRequiredFields` alone is not it — before the spec loads, every
 * required field reads as missing, so marking them all `aria-invalid` on
 * first paint would announce a form full of errors the user has not done
 * anything about. The state flips when an install is actually refused
 * for a missing field, which is the moment it became true and the moment
 * the user needs to hear about it.
 */
function fieldInvalid(capabilityId: string, fieldName: string): boolean {
  return (
    stateFor(capabilityId).errorKind === 'validation'
    && missingRequiredFields(capabilityId).includes(fieldName)
  )
}

/** The hint and the error share one control's `aria-describedby`. */
function fieldDescribedBy(capabilityId: string, field: { name: string; description?: string }): string | undefined {
  const ids = [`${fieldId(capabilityId, field.name)}-hint`]
  if (fieldInvalid(capabilityId, field.name)) ids.push(`${fieldId(capabilityId, field.name)}-error`)
  return ids.join(' ')
}

function fieldId(capabilityId: string, fieldName: string): string {
  return `capability-field-${capabilityId}-${fieldName}`
}

function setConfigValue(state: CapabilityCardState, name: string, value: CapabilityConfigValue) {
  state.configValues[name] = value
}

function setNumberValue(state: CapabilityCardState, name: string, raw: string) {
  const parsed = Number(raw)
  state.configValues[name] = raw === '' || Number.isNaN(parsed) ? 0 : parsed
}

function originLabel(origin: CapabilityOrigin) {
  return origin === 'local' ? t('capabilityOriginLocal') : t('capabilityOriginCommunity')
}

function originTone(origin: CapabilityOrigin): 'warning' | 'info' {
  return origin === 'local' ? 'warning' : 'info'
}

function cardErrorText(state: CapabilityCardState): string | null {
  if (!state.error) return null
  if (state.errorKind === 'validation') return state.error
  if (state.errorKind === 'install') return t('capabilityInstallFailed', { error: state.error })
  return t('capabilityActionFailed', { error: state.error })
}

async function onInstall(capability: CapabilitySummary, force = false) {
  const state = stateFor(capability.id)
  if (state.busy || state.verifyBusy) return
  await modules.ensureSpec(capability)
  const missing = missingRequiredFields(capability.id)
  if (missing.length) {
    state.error = t('capabilityRequiredMissing', { fields: missing.join(', ') })
    state.errorKind = 'validation'
    return
  }
  // A plain install is refused by the backend for an unsupported game
  // build, so the card routes the user through the explicit override
  // instead of letting them click into an error.
  if (!force && compatibilityBlocks(capability.id)) return
  await install(capability, { force })
}

const pendingRemoval = ref<CapabilitySummary | null>(null)

async function onRemove(capability: CapabilitySummary) {
  const state = stateFor(capability.id)
  if (state.busy) return
  // Removing a capability writes into the game folder. The transaction
  // store can undo it, but "can be undone" is not the same as "asked
  // first" — so ask first.
  pendingRemoval.value = capability
}

async function confirmRemoval() {
  const capability = pendingRemoval.value
  if (!capability) return
  pendingRemoval.value = null
  await uninstall(capability)
}

async function onVerify(capability: CapabilitySummary) {
  const state = stateFor(capability.id)
  if (state.busy || state.verifyBusy) return
  await verify(capability)
}

// The AI dialog dispatches this after a successful save so a mod saved
// mid-session shows up here without a restart.
function handleCapabilitySaved() {
  void modules.refresh()
}

onMounted(() => {
  void ensureLoaded()
  window.addEventListener('moddin:capability-saved', handleCapabilitySaved)
})

onUnmounted(() => {
  window.removeEventListener('moddin:capability-saved', handleCapabilitySaved)
})
</script>

<template>
  <section
    v-if="visibleCapabilities.length > 0 || loadError"
    class="capability-mods"
    :aria-busy="loading"
  >
    <div class="section-heading">
      <div>
        <h2>{{ t('capabilityModsHeading') }}</h2>
        <p>{{ t('capabilityModsSubtitle') }}</p>
      </div>
    </div>

    <div v-if="loadError && !visibleCapabilities.length" class="callout callout-danger" role="alert">
      <AppIcon class="callout-icon" name="alert" />
      <div>
        <strong>{{ t('capabilityLoadFailed', { error: loadError }) }}</strong>
      </div>
    </div>

    <EmptyState
      v-else-if="loading && !visibleCapabilities.length"
      busy
      :description="t('capabilityLoading')"
    />

    <div v-if="visibleCapabilities.length" class="mod-grid">
      <!--
        UX-29: the cell is the region that changes while an install or a
        check runs, so it is the region that says so. Without this the
        card silently swaps "Install" for a spinner and the change is
        only visible.
      -->
      <div
        v-for="capability in visibleCapabilities"
        :key="capability.id"
        class="capability-cell"
        :aria-busy="stateFor(capability.id).busy || stateFor(capability.id).verifyBusy"
      >
        <div
          v-if="blockedCompatibility(stateFor(capability.id)) && !isInstalled(capability.id)"
          class="callout callout-warning"
          role="alert"
        >
          <AppIcon class="callout-icon" name="alert" />
          <div class="capability-compat">
            <strong>{{ t('capabilityCompatBlocked') }}</strong>
            <small v-if="blockedCompatibility(stateFor(capability.id))?.detail">
              {{ blockedCompatibility(stateFor(capability.id))?.detail }}
            </small>
            <button
              type="button"
              class="btn btn-sm"
              :class="{ 'is-loading': stateFor(capability.id).busy }"
              :disabled="stateFor(capability.id).busy || stateFor(capability.id).verifyBusy || dirsMissing"
              @click="onInstall(capability, true)"
            >
              {{ t('capabilityCompatForce') }}
            </button>
          </div>
        </div>

        <div
          v-else-if="blockedCompatibility(stateFor(capability.id)) && isInstalled(capability.id)"
          class="callout callout-warning"
        >
          <AppIcon class="callout-icon" name="alert" />
          <div>
            <strong>{{ t('capabilityCompatForced') }}</strong>
          </div>
        </div>

        <div
          v-if="installedDependencyNames(capability.id)"
          class="callout callout-info"
        >
          <AppIcon class="callout-icon" name="check" />
          <div>
            <strong>
              {{ t('capabilityDependenciesInstalled', { modules: installedDependencyNames(capability.id) }) }}
            </strong>
          </div>
        </div>

        <div
          v-if="cardErrorText(stateFor(capability.id))"
          class="callout callout-danger"
          role="alert"
        >
          <AppIcon class="callout-icon" name="alert" />
          <div>
            <strong>{{ cardErrorText(stateFor(capability.id)) }}</strong>
          </div>
        </div>

        <ModuleCard
          :name="capability.displayName"
          :description="descriptionOf(capability)"
          :state="cardState(capability.id)"
          :tag="{ label: originLabel(capability.origin), tone: originTone(capability.origin) }"
          :action-label="isInstalled(capability.id) ? t('actionReinstall') : t('actionInstall')"
          :action-primary="!isInstalled(capability.id)"
          :action-busy="stateFor(capability.id).busy"
          :action-disabled="dirsMissing || stateFor(capability.id).busy || stateFor(capability.id).verifyBusy || (!isInstalled(capability.id) && compatibilityBlocks(capability.id))"
          :blocked-reason="blockedReasonText"
          :verification="cardVerification(stateFor(capability.id))"
          :verify-busy="stateFor(capability.id).verifyBusy"
          :remove-label="isInstalled(capability.id) ? t('actionRemove') : null"
          @action="onInstall(capability)"
          @verify="onVerify(capability)"
          @remove="onRemove(capability)"
        >
          <template #details>
            <div v-if="safetyNotesOf(capability.id).length" class="callout callout-warning">
              <AppIcon class="callout-icon" name="alert" />
              <div>
                <strong>{{ t('aiAssistantSafetyNotes') }}</strong>
                <ul class="note-list capability-notes">
                  <li v-for="(note, index) in safetyNotesOf(capability.id)" :key="index">{{ note }}</li>
                </ul>
              </div>
            </div>

            <section v-if="schemaOf(capability.id).length" class="capability-detail">
              <h5 class="capability-detail-title">{{ t('capabilityConfigTitle') }}</h5>
              <div class="capability-fields">
                <label v-for="field in schemaOf(capability.id)" :key="field.name" class="field">
                  <span>
                    {{ field.name }}<em v-if="field.required" class="capability-required" aria-hidden="true">*</em>
                  </span>
                  <select
                    v-if="field.type === 'enum'"
                    class="select"
                    :id="fieldId(capability.id, field.name)"
                    :value="String(configValue(stateFor(capability.id), field.name))"
                    :aria-required="field.required ? 'true' : undefined"
                    :aria-invalid="fieldInvalid(capability.id, field.name) ? 'true' : undefined"
                    :aria-describedby="fieldDescribedBy(capability.id, field)"
                    @change="setConfigValue(stateFor(capability.id), field.name, ($event.target as HTMLSelectElement).value)"
                  >
                    <option v-for="option in field.enumValues ?? []" :key="option" :value="option">
                      {{ option }}
                    </option>
                  </select>
                  <input
                    v-else-if="field.type === 'boolean'"
                    type="checkbox"
                    class="capability-checkbox"
                    :id="fieldId(capability.id, field.name)"
                    :checked="Boolean(configValue(stateFor(capability.id), field.name))"
                    @change="setConfigValue(stateFor(capability.id), field.name, ($event.target as HTMLInputElement).checked)"
                  />
                  <input
                    v-else-if="field.type === 'number'"
                    type="number"
                    class="input"
                    :id="fieldId(capability.id, field.name)"
                    :value="Number(configValue(stateFor(capability.id), field.name))"
                    :aria-required="field.required ? 'true' : undefined"
                    :aria-invalid="fieldInvalid(capability.id, field.name) ? 'true' : undefined"
                    :aria-describedby="fieldDescribedBy(capability.id, field)"
                    @input="setNumberValue(stateFor(capability.id), field.name, ($event.target as HTMLInputElement).value)"
                  />
                  <input
                    v-else
                    type="text"
                    class="input"
                    :id="fieldId(capability.id, field.name)"
                    :value="String(configValue(stateFor(capability.id), field.name))"
                    :aria-required="field.required ? 'true' : undefined"
                    :aria-invalid="fieldInvalid(capability.id, field.name) ? 'true' : undefined"
                    :aria-describedby="fieldDescribedBy(capability.id, field)"
                    @input="setConfigValue(stateFor(capability.id), field.name, ($event.target as HTMLInputElement).value)"
                  />
                  <small v-if="field.description" :id="`${fieldId(capability.id, field.name)}-hint`">
                    {{ field.description }}
                  </small>
                  <small
                    v-if="fieldInvalid(capability.id, field.name)"
                    :id="`${fieldId(capability.id, field.name)}-error`"
                    class="capability-field-error"
                    role="alert"
                  >
                    {{ t('capabilityFieldRequired') }}
                  </small>
                </label>
              </div>
            </section>
          </template>
        </ModuleCard>
      </div>
    </div>

    <ConfirmDialog
      v-if="pendingRemoval"
      :title="t('capabilityRemoveConfirmTitle')"
      :description="t('capabilityRemoveConfirmDescription', { name: pendingRemoval.displayName })"
      :confirm-label="t('actionRemove')"
      :cancel-label="t('actionCancel')"
      :details="[t('capabilityRemoveDetailFiles'), t('capabilityRemoveDetailUndo')]"
      :footnote="t('capabilityRemoveFootnote')"
      @close="pendingRemoval = null"
      @confirm="confirmRemoval"
    />
  </section>
</template>

<style scoped>
.capability-mods { display: grid; gap: var(--moddin-space-5); }
.section-heading { display: flex; flex-wrap: wrap; align-items: flex-end; justify-content: space-between; gap: var(--moddin-space-3); }
.section-heading p { margin-top: 2px; color: var(--moddin-text-muted); font-size: var(--moddin-text-md); }

.mod-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: var(--moddin-space-3); }
.capability-cell { display: flex; flex-direction: column; gap: var(--moddin-space-2); }
.capability-cell .module-card { flex: 1; }

.capability-detail { display: grid; gap: var(--moddin-space-2); }
.capability-detail-title { margin: 0; color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }
.capability-detail-heading { display: flex; align-items: baseline; justify-content: space-between; gap: var(--moddin-space-2); }
.capability-detail-heading small { color: var(--moddin-text-faint); font-size: var(--moddin-text-xs); }

.capability-fields { display: grid; gap: var(--moddin-space-3); }
.capability-required { margin-left: 2px; color: var(--moddin-danger); font-style: normal; }
.capability-checkbox { width: 15px; height: 15px; margin-top: 3px; accent-color: var(--moddin-accent); }
.capability-field-error { color: var(--moddin-danger); }

.capability-compat { display: grid; justify-items: start; gap: var(--moddin-space-2); }
.capability-compat small { color: var(--moddin-text-soft); font-size: var(--moddin-text-xs); }
</style>
