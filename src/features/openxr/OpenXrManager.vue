<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import { openXrCopyForLocale } from './copy'
import { useOpenXrManager } from './useOpenXrManager'

const { locale } = useI18n()
const copy = computed(() => openXrCopyForLocale(locale.value))

const friendlyError = computed<{ title: string; why: string }>(() => {
  const raw = error.value ?? ''
  const c = copy.value
  if (/admin|UAC|denied|permission|elevation|0x80070005|0x80070252/i.test(raw)) {
    return { title: c.errorUacTitle, why: c.errorUacWhy }
  }
  return { title: c.errorGenericTitle, why: c.errorGenericWhy }
})

const {
  open,
  dialogElement,
  loading,
  busyAction,
  error,
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
    <div v-if="open" class="dialog-backdrop" @click.self="closeManager">
      <section
        ref="dialogElement"
        class="dialog dialog-lg"
        role="dialog"
        aria-modal="true"
        :aria-label="copy.title"
        tabindex="-1"
      >
        <header class="dialog-header">
          <div>
            <h2>{{ copy.title }}</h2>
            <p class="dialog-description">{{ copy.subtitle }}</p>
          </div>
          <button class="btn btn-icon" type="button" :aria-label="copy.close" @click="closeManager">
            <AppIcon name="close" :size="18" />
          </button>
        </header>

        <div class="dialog-body">
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

          <div v-if="error" class="callout callout-danger" role="alert">
            <strong>{{ friendlyError.title }}</strong>
            <p>{{ friendlyError.why }}</p>
            <details class="callout-raw">
              <summary>{{ copy.errorRawToggle }}</summary>
              <code>{{ error }}</code>
            </details>
          </div>
          <div v-if="loading && !state" class="empty-state"><span class="spinner" /></div>

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
              <div v-if="!state.runtimes.length" class="empty-state">{{ copy.noRuntimes }}</div>
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
                      @click="setSystemRuntime(runtime.manifestPath)"
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
        </div>
      </section>
    </div>
  </Teleport>
</template>

<style scoped src="./openxr-manager.css"></style>
