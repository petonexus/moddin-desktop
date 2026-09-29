<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import ModuleCard from '../library/ModuleCard.vue'
import type { TransactionRecord } from '../../types/transaction'
import type { CapabilityOrigin, CapabilitySummary } from '../../types/capability'
import { useCapabilityModules } from './useCapabilityModules'
import type { CapabilityCardState, CapabilityConfigValue } from './types'

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

/** Ids auto-installed by the backend as dependencies of the last install. */
function installedDependencyNames(capabilityId: string): string {
  return stateFor(capabilityId).installedDependencies.join(', ')
}

function configValue(state: CapabilityCardState, name: string): CapabilityConfigValue {
  return state.configValues[name] ?? ''
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

async function onRemove(capability: CapabilitySummary) {
  const state = stateFor(capability.id)
  if (state.busy) return
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

    <div v-if="visibleCapabilities.length" class="mod-grid">
      <div v-for="capability in visibleCapabilities" :key="capability.id" class="capability-cell">
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
          :description="capability.id"
          :state="isInstalled(capability.id) ? 'active' : 'available'"
          :tag="{ label: originLabel(capability.origin), tone: originTone(capability.origin) }"
          :action-label="isInstalled(capability.id) ? t('actionReinstall') : t('actionInstall')"
          :action-primary="!isInstalled(capability.id)"
          :action-busy="stateFor(capability.id).busy"
          :action-disabled="dirsMissing || stateFor(capability.id).busy || stateFor(capability.id).verifyBusy || (!isInstalled(capability.id) && compatibilityBlocks(capability.id))"
          :blocked-reason="blockedReasonText"
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
                    :value="String(configValue(stateFor(capability.id), field.name))"
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
                    :checked="Boolean(configValue(stateFor(capability.id), field.name))"
                    @change="setConfigValue(stateFor(capability.id), field.name, ($event.target as HTMLInputElement).checked)"
                  />
                  <input
                    v-else-if="field.type === 'number'"
                    type="number"
                    class="input"
                    :value="Number(configValue(stateFor(capability.id), field.name))"
                    @input="setNumberValue(stateFor(capability.id), field.name, ($event.target as HTMLInputElement).value)"
                  />
                  <input
                    v-else
                    type="text"
                    class="input"
                    :value="String(configValue(stateFor(capability.id), field.name))"
                    @input="setConfigValue(stateFor(capability.id), field.name, ($event.target as HTMLInputElement).value)"
                  />
                  <small v-if="field.description">{{ field.description }}</small>
                </label>
              </div>
            </section>

            <section v-if="stateFor(capability.id).verification" class="capability-detail">
              <div class="capability-detail-heading">
                <h5 class="capability-detail-title">{{ t('checklistTitle') }}</h5>
                <small>{{ stateFor(capability.id).verification?.summary }}</small>
              </div>
              <ul v-if="stateFor(capability.id).verification?.checks.length" class="capability-checks">
                <li
                  v-for="(check, index) in stateFor(capability.id).verification?.checks ?? []"
                  :key="check.id ?? `${check.label}-${index}`"
                  :class="check.passed ? 'ok' : 'fail'"
                >
                  <AppIcon :name="check.passed ? 'check' : 'alert'" :size="13" />
                  <span>
                    {{ check.label }}
                    <small v-if="check.detail">{{ check.detail }}</small>
                  </span>
                </li>
              </ul>
              <p v-else class="capability-hint">
                {{ stateFor(capability.id).verification?.summary }}
              </p>
            </section>
          </template>
        </ModuleCard>
      </div>
    </div>
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

.capability-checks { display: grid; gap: 6px; margin: 0; padding: 0; list-style: none; }
.capability-checks li { display: flex; align-items: flex-start; gap: var(--moddin-space-2); color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }
.capability-checks li .app-icon { margin-top: 2px; }
.capability-checks li.ok .app-icon { color: var(--moddin-success); }
.capability-checks li.fail .app-icon { color: var(--moddin-warning); }
.capability-checks small { display: block; color: var(--moddin-text-faint); font-size: var(--moddin-text-xs); }

.capability-hint { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }

.capability-compat { display: grid; justify-items: start; gap: var(--moddin-space-2); }
.capability-compat small { color: var(--moddin-text-soft); font-size: var(--moddin-text-xs); }
</style>
