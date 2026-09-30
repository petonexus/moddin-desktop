<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import BaseDialog from '../../components/ui/BaseDialog.vue'
import ConfirmDialog from '../../components/ui/ConfirmDialog.vue'
import ErrorCallout from '../../components/ui/ErrorCallout.vue'
import EmptyState from '../../components/ui/EmptyState.vue'
import { useFriendlyError, type FriendlyErrorRule } from '../../composables/useFriendlyError'
import { openXrCopyForLocale } from './copy'
import { useOpenXrManager } from './useOpenXrManager'

const { locale } = useI18n()
const copy = computed(() => openXrCopyForLocale(locale.value))

const {
  open,
  loading,
  busyAction,
  error,
  errorContext,
  state,
  selectedGameId,
  gameChoices,
  isSelectedForGame,
  runtimeUsable,
  inspect,
  openManager,
  closeManager,
  setGameRuntime,
  setSystemRuntime,
} = useOpenXrManager()

const errorRules = computed<FriendlyErrorRule[]>(() => {
  const c = copy.value
  return [
    { match: /admin|UAC|denied|permission|elevation|0x80070005|0x80070252/i, title: c.errorUacTitle, why: c.errorUacWhy, showRaw: true },
    { title: c.errorGenericTitle, why: c.errorGenericWhy, showRaw: true },
  ]
})

const friendlyError = useFriendlyError({ error, rules: () => errorRules.value, context: errorContext })

// Making a runtime the Windows-wide default writes to HKLM and prompts
// for elevation. It changes every VR game on the machine, not the one
// on screen, so it gets a confirmation of its own rather than living
// in a passive callout at the bottom of the dialog.
const pendingSystemRuntime = ref<{ name: string; manifestPath: string } | null>(null)

function askSetSystemRuntime(runtime: { name: string; manifestPath: string }) {
  pendingSystemRuntime.value = runtime
}

async function confirmSetSystemRuntime() {
  const runtime = pendingSystemRuntime.value
  if (!runtime) return
  pendingSystemRuntime.value = null
  await setSystemRuntime(runtime.manifestPath)
}

function effectiveSourceLabel() {
  if (state.value?.effectiveSource === 'game') return copy.value.sourceGame
  if (state.value?.effectiveSource === 'system') return copy.value.sourceSystem
  return copy.value.sourceNone
}
</script>

<template>
  <button class="nav-item" type="button" @click="openManager">
    <AppIcon name="vr" />
    <span>{{ copy.button }}</span>
  </button>

  <Teleport to="body">
    <BaseDialog
      v-if="open"
      size="lg"
      :title="copy.title"
      :description="copy.subtitle"
      @close="closeManager"
    >
      <div class="openxr-toolbar">
        <label class="field">
          <span>{{ copy.selectGame }}</span>
          <select v-model="selectedGameId" class="select">
            <option value="">{{ copy.systemOnly }}</option>
            <option v-for="game in gameChoices" :key="game.gameId" :value="game.gameId">{{ game.label }}</option>
          </select>
        </label>
        <button class="btn btn-sm" type="button" :disabled="loading || busyAction !== null" @click="inspect">
          <AppIcon name="refresh" :size="14" />
          {{ copy.refresh }}
        </button>
      </div>

      <ErrorCallout :error="friendlyError" />
      <EmptyState v-if="loading && !state" busy />

      <template v-if="state">
        <div class="fact-grid">
          <div class="fact">
            <span>{{ copy.windowsRuntime }}</span>
            <strong>{{ state.activeRuntimeName ?? copy.noRuntime }}</strong>
          </div>
          <div v-if="selectedGameId" class="fact">
            <span>{{ copy.effectiveRuntime }} · {{ effectiveSourceLabel() }}</span>
            <strong>{{ state.effectiveRuntimeName ?? copy.noRuntime }}</strong>
          </div>
        </div>

        <div v-if="state.warnings.length" class="callout callout-warning">
          <AppIcon class="callout-icon" name="alert" />
          <ul class="note-list">
            <li v-for="warning in state.warnings" :key="warning">{{ warning }}</li>
          </ul>
        </div>

        <section class="dialog-section">
          <h3>{{ copy.runtimes }}</h3>
          <EmptyState v-if="!state.runtimes.length" :description="copy.noRuntimes" />
          <div v-else class="openxr-runtime-list">
            <article v-for="runtime in state.runtimes" :key="runtime.manifestPath" class="openxr-runtime">
              <div class="openxr-runtime-copy">
                <div class="openxr-runtime-title">
                  <strong>{{ runtime.name }}</strong>
                  <span v-if="runtime.active" class="badge badge-success">{{ copy.active }}</span>
                  <span v-if="isSelectedForGame(runtime)" class="badge badge-accent">{{ copy.selected }}</span>
                  <span v-if="!runtime.enabled" class="badge">{{ copy.disabled }}</span>
                  <span v-if="!runtime.manifestExists" class="badge badge-danger">{{ copy.missingManifest }}</span>
                  <span v-else-if="!runtime.libraryExists" class="badge badge-warning">{{ copy.missingLibrary }}</span>
                </div>
                <p class="path-text">{{ runtime.manifestPath }}</p>
              </div>
              <div class="openxr-runtime-actions">
                <button
                  v-if="selectedGameId"
                  class="btn btn-sm"
                  :class="{ 'is-loading': busyAction === `game:${runtime.manifestPath}` }"
                  type="button"
                  :disabled="!runtimeUsable(runtime) || busyAction !== null || isSelectedForGame(runtime)"
                  @click="setGameRuntime(runtime.manifestPath)"
                >
                  {{ busyAction === `game:${runtime.manifestPath}` ? copy.applying : copy.useForGame }}
                </button>
                <button
                  class="btn btn-sm"
                  :class="{ 'btn-primary': !selectedGameId, 'is-loading': busyAction === `system:${runtime.manifestPath}` }"
                  type="button"
                  :disabled="!runtimeUsable(runtime) || busyAction !== null || runtime.active"
                  @click="askSetSystemRuntime(runtime)"
                >
                  {{ busyAction === `system:${runtime.manifestPath}` ? copy.applying : copy.makeSystem }}
                </button>
              </div>
            </article>
          </div>
        </section>

        <div class="callout callout-info">
          <AppIcon class="callout-icon" name="info" />
          <div>
            <p v-if="selectedGameId">{{ copy.gameHint }}</p>
            <p>{{ copy.systemHint }}</p>
          </div>
        </div>
        <div v-if="selectedGameId && state.gameOverride !== null">
          <button class="btn btn-ghost btn-sm" type="button" :disabled="busyAction !== null" @click="setGameRuntime(null)">
            {{ copy.systemDefault }}
          </button>
        </div>
      </template>
    </BaseDialog>

    <ConfirmDialog
      v-if="pendingSystemRuntime"
      :title="copy.makeSystemConfirmTitle"
      :description="copy.makeSystemConfirmDescription"
      :confirm-label="copy.makeSystem"
      :cancel-label="copy.cancelAction"
      :details="[copy.makeSystemConfirmScope, copy.makeSystemConfirmAdmin]"
      @close="pendingSystemRuntime = null"
      @confirm="confirmSetSystemRuntime"
    />
  </Teleport>
</template>

<style scoped src="./openxr-manager.css"></style>
