<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { openXrCopyForLocale } from './copy'
import { useOpenXrManager } from './useOpenXrManager'

const { locale } = useI18n()
const copy = computed(() => openXrCopyForLocale(locale.value))

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
  <button class="openxr-fab" type="button" @click="openManager">
    <span class="openxr-dot"></span>
    {{ copy.button }}
  </button>

  <div v-if="open" class="openxr-backdrop" @click.self="closeManager">
    <section
      ref="dialogElement"
      class="openxr-panel"
      role="dialog"
      aria-modal="true"
      :aria-label="copy.title"
      tabindex="-1"
    >
      <header class="openxr-header">
        <div>
          <small>OPENXR</small>
          <h2>{{ copy.title }}</h2>
          <p>{{ copy.subtitle }}</p>
        </div>
        <button class="openxr-icon-button" type="button" :aria-label="copy.close" @click="closeManager">×</button>
      </header>

      <div class="openxr-toolbar">
        <label>
          <span>{{ copy.selectGame }}</span>
          <select v-model="selectedGameId">
            <option value="">{{ copy.systemOnly }}</option>
            <option v-for="game in gameChoices" :key="game.gameId" :value="game.gameId">{{ game.label }}</option>
          </select>
        </label>
        <button class="openxr-secondary" type="button" :disabled="loading || busyAction !== null" @click="inspect">
          {{ copy.refresh }}
        </button>
      </div>

      <div v-if="error" class="openxr-error">{{ error }}</div>
      <div v-if="loading && !state" class="openxr-empty">{{ copy.refresh }}…</div>

      <template v-if="state">
        <div class="openxr-summary">
          <div>
            <span>{{ copy.windowsRuntime }}</span>
            <strong>{{ state.activeRuntimeName ?? copy.noRuntime }}</strong>
            <code v-if="state.activeRuntime">{{ state.activeRuntime }}</code>
          </div>
          <div v-if="selectedGameId">
            <span>{{ copy.effectiveRuntime }} · {{ effectiveSourceLabel() }}</span>
            <strong>{{ state.effectiveRuntimeName ?? copy.noRuntime }}</strong>
            <code v-if="state.effectiveRuntime">{{ state.effectiveRuntime }}</code>
          </div>
        </div>

        <div v-if="selectedGameId" class="openxr-note">
          <strong>{{ copy.gameOverride }}</strong>
          <p>{{ copy.gameHint }}</p>
          <button
            class="openxr-secondary"
            type="button"
            :disabled="busyAction !== null || state.gameOverride === null"
            @click="setGameRuntime(null)"
          >
            {{ copy.systemDefault }}
          </button>
        </div>

        <div class="openxr-note">
          <strong>Windows</strong>
          <p>{{ copy.systemHint }}</p>
          <small>{{ copy.uacHint }}</small>
        </div>

        <div v-if="state.warnings.length" class="openxr-warnings">
          <p v-for="warning in state.warnings" :key="warning">{{ warning }}</p>
        </div>

        <div class="openxr-section-title">
          <h3>{{ copy.runtimes }}</h3>
          <span>{{ state.runtimes.length }}</span>
        </div>

        <div v-if="!state.runtimes.length" class="openxr-empty">{{ copy.noRuntimes }}</div>
        <div v-else class="openxr-runtime-list">
          <article v-for="runtime in state.runtimes" :key="runtime.manifestPath" class="openxr-runtime">
            <div class="openxr-runtime-heading">
              <div>
                <strong>{{ runtime.name }}</strong>
                <code>{{ runtime.manifestPath }}</code>
              </div>
              <div class="openxr-badges">
                <span v-if="runtime.active" class="good">{{ copy.active }}</span>
                <span v-if="isSelectedForGame(runtime)" class="selected">{{ copy.selected }}</span>
                <span v-if="!runtime.enabled" class="bad">{{ copy.disabled }}</span>
                <span v-if="!runtime.manifestExists" class="bad">{{ copy.missingManifest }}</span>
                <span v-else-if="!runtime.libraryExists" class="warn">{{ copy.missingLibrary }}</span>
              </div>
            </div>

            <div class="openxr-runtime-actions">
              <button
                v-if="selectedGameId"
                class="openxr-secondary"
                type="button"
                :disabled="!runtimeUsable(runtime) || busyAction !== null || isSelectedForGame(runtime)"
                @click="setGameRuntime(runtime.manifestPath)"
              >
                {{ busyAction === `game:${runtime.manifestPath}` ? copy.applying : copy.useForGame }}
              </button>
              <button
                class="openxr-primary"
                type="button"
                :disabled="!runtimeUsable(runtime) || busyAction !== null || runtime.active"
                @click="setSystemRuntime(runtime.manifestPath)"
              >
                {{ busyAction === `system:${runtime.manifestPath}` ? copy.applying : copy.makeSystem }}
              </button>
            </div>
          </article>
        </div>
      </template>
    </section>
  </div>
</template>

<style scoped src="./openxr-manager.css"></style>
