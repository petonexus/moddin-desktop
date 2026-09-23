<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAiAssistant } from '../../composables/useAiAssistant'
import { useDialogLifecycle } from '../../composables/useDialogLifecycle'
import { listCapabilitiesForAssistant } from '../../features/ai-assistant/service'
import type { CapabilitySummary } from '../../types/capability'

const props = defineProps<{
  open: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const { t } = useI18n()
const ai = useAiAssistant()

const installedCapabilities = ref<CapabilitySummary[]>([])
const selectedCapabilityId = ref<string>('')
const intent = ref('')
const step = ref<'menu' | 'improve-pick' | 'improve-prompt'>('menu')
const busy = ref(false)

const dialogElement = ref<HTMLElement | null>(null)
const dialogOpen = computed({
  get: () => props.open,
  set: () => {
    /* controlled by parent */
  },
})
const { closeDialog } = useDialogLifecycle(dialogOpen)

const availableInstalled = computed(() =>
  installedCapabilities.value.filter(
    (c) => c.origin === 'builtIn' || c.origin === 'local',
  ),
)

function close() {
  emit('close')
}

function closeOnBackdrop(event: MouseEvent) {
  if (event.target === event.currentTarget) {
    close()
  }
}

function reset() {
  step.value = 'menu'
  selectedCapabilityId.value = ''
  intent.value = ''
  busy.value = false
}

async function loadInstalledCapabilities() {
  try {
    installedCapabilities.value = await listCapabilitiesForAssistant()
  } catch {
    // Non-fatal — the improve picker just won't have entries.
  }
}

async function pickPath(path: 'author' | 'improve' | 'diagnose') {
  reset()
  if (path === 'improve') {
    await loadInstalledCapabilities()
    step.value = 'improve-pick'
    return
  }
  if (path === 'author') {
    await openAuthor()
    return
  }
  if (path === 'diagnose') {
    await openDiagnose()
  }
}

async function openAuthor() {
  busy.value = true
  try {
    await ai.openFor({
      mode: 'author',
      gameId: readSelectedAppId(),
      gameName: readSelectedGameName(),
      intent: t('contributeAuthorIntent'),
    })
    close()
  } finally {
    busy.value = false
  }
}

async function openDiagnose() {
  busy.value = true
  try {
    await ai.openFor({
      mode: 'diagnose',
      gameId: readSelectedAppId(),
      gameName: readSelectedGameName(),
      intent: t('contributeDiagnoseIntent'),
      errorMessage: t('contributeDiagnosePlaceholder'),
      errorSource: 'user',
    })
    close()
  } finally {
    busy.value = false
  }
}

async function confirmImprove() {
  const capId = selectedCapabilityId.value
  if (!capId) return
  busy.value = true
  try {
    const cap = installedCapabilities.value.find((c) => c.id === capId)
    await ai.openFor({
      mode: 'improve',
      gameId: readSelectedAppId(),
      gameName: readSelectedGameName(),
      capabilityId: capId,
      intent: intent.value.trim() || t('contributeImproveDefaultIntent', { capability: cap?.displayName ?? capId }),
    })
    close()
  } finally {
    busy.value = false
  }
}

function readSelectedAppId(): string | null {
  try {
    return window.localStorage.getItem('moddin-selected-appId')
  } catch {
    return null
  }
}

function readSelectedGameName(): string | null {
  try {
    return window.localStorage.getItem('moddin-selected-game-name')
  } catch {
    return null
  }
}

onMounted(loadInstalledCapabilities)
</script>

<template>
  <div v-if="open" class="ct-backdrop" @click="closeOnBackdrop">
    <section
      ref="dialogElement"
      class="ct-panel"
      role="dialog"
      aria-modal="true"
      :aria-label="t('contributeTitle')"
      tabindex="-1"
    >
      <header class="ct-header">
        <div>
          <small>CONTRIBUIR</small>
          <h2>{{ t('contributeTitle') }}</h2>
          <p>{{ t('contributeSubtitle') }}</p>
        </div>
        <button
          class="ct-icon-button"
          type="button"
          :aria-label="t('contributeClose')"
          @click="close"
        >
          ×
        </button>
      </header>

      <!-- Step 1: three big choices -->
      <div v-if="step === 'menu'" class="ct-grid">
        <button
          class="ct-card"
          type="button"
          :disabled="busy"
          @click="pickPath('author')"
        >
          <span class="ct-card-icon" aria-hidden="true">✨</span>
          <strong>{{ t('contributeAuthorTitle') }}</strong>
          <p>{{ t('contributeAuthorBody') }}</p>
        </button>
        <button
          class="ct-card"
          type="button"
          :disabled="busy"
          @click="pickPath('improve')"
        >
          <span class="ct-card-icon" aria-hidden="true">🔧</span>
          <strong>{{ t('contributeImproveTitle') }}</strong>
          <p>{{ t('contributeImproveBody') }}</p>
        </button>
        <button
          class="ct-card"
          type="button"
          :disabled="busy"
          @click="pickPath('diagnose')"
        >
          <span class="ct-card-icon" aria-hidden="true">🩺</span>
          <strong>{{ t('contributeDiagnoseTitle') }}</strong>
          <p>{{ t('contributeDiagnoseBody') }}</p>
        </button>
      </div>

      <!-- Step 2: pick a capability to improve -->
      <div v-else-if="step === 'improve-pick'" class="ct-step">
        <label class="ct-field">
          <span>{{ t('contributeImprovePickerLabel') }}</span>
          <select v-model="selectedCapabilityId" :disabled="busy || availableInstalled.length === 0">
            <option value="" disabled>{{ t('contributeImprovePickerPlaceholder') }}</option>
            <option v-for="cap in availableInstalled" :key="cap.id" :value="cap.id">
              {{ cap.displayName }} ({{ cap.id }})
            </option>
          </select>
        </label>
        <label class="ct-field">
          <span>{{ t('contributeImproveIntentLabel') }}</span>
          <textarea
            v-model="intent"
            rows="3"
            :placeholder="t('contributeImproveIntentPlaceholder')"
            :disabled="busy"
          />
        </label>
        <div class="ct-actions">
          <button class="ct-secondary" type="button" :disabled="busy" @click="step = 'menu'">
            {{ t('contributeBack') }}
          </button>
          <button
            class="ct-primary"
            type="button"
            :disabled="busy || !selectedCapabilityId"
            @click="confirmImprove"
          >
            {{ t('contributeImproveContinue') }}
          </button>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.ct-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  z-index: 9050;
  display: flex;
  align-items: center;
  justify-content: center;
}

.ct-panel {
  width: min(680px, 96vw);
  max-height: 86vh;
  overflow: auto;
  background: var(--moddin-surface, #141823);
  color: var(--moddin-text, #e8ecf2);
  border-radius: 12px;
  padding: 1.25rem 1.5rem 1.5rem;
  box-shadow: 0 24px 48px rgba(0, 0, 0, 0.6);
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
}

.ct-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
}

.ct-header small {
  letter-spacing: 0.08em;
  color: var(--moddin-accent, #7aa2f7);
  font-size: 0.75rem;
  font-weight: 700;
}

.ct-header h2 {
  margin: 0.1rem 0 0.2rem;
  font-size: 1.05rem;
}

.ct-header p {
  margin: 0;
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.82rem;
}

.ct-icon-button {
  background: transparent;
  border: 1px solid var(--moddin-border, #3a4252);
  color: inherit;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  cursor: pointer;
  font-size: 1.1rem;
}

.ct-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 0.6rem;
}

@media (max-width: 640px) {
  .ct-grid {
    grid-template-columns: 1fr;
  }
}

.ct-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 0.35rem;
  background: var(--moddin-surface-2, #1a1f29);
  border: 1px solid var(--moddin-border, #3a4252);
  border-radius: 8px;
  padding: 0.85rem 0.9rem;
  color: inherit;
  text-align: left;
  cursor: pointer;
  transition: border-color 120ms ease, transform 120ms ease;
}

.ct-card:hover:not(:disabled) {
  border-color: var(--moddin-accent, #7aa2f7);
  transform: translateY(-1px);
}

.ct-card:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.ct-card-icon {
  font-size: 1.5rem;
}

.ct-card strong {
  font-size: 0.95rem;
}

.ct-card p {
  margin: 0;
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.78rem;
  line-height: 1.35;
}

.ct-step {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.ct-field {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  font-size: 0.85rem;
}

.ct-field > span {
  font-weight: 600;
  color: var(--moddin-muted, #8c93a3);
}

.ct-field select,
.ct-field textarea {
  background: var(--moddin-surface-2, #1a1f29);
  color: inherit;
  border: 1px solid var(--moddin-border, #3a4252);
  border-radius: 6px;
  padding: 0.5rem 0.65rem;
  font-size: 0.85rem;
  resize: vertical;
}

.ct-field textarea {
  font-family: inherit;
}

.ct-actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
}

.ct-primary {
  background: var(--moddin-accent, #7aa2f7);
  color: #0c0e15;
  border: none;
  border-radius: 6px;
  padding: 0.45rem 1rem;
  font-weight: 600;
  cursor: pointer;
}

.ct-primary:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.ct-secondary {
  background: transparent;
  border: 1px solid var(--moddin-accent, #7aa2f7);
  color: var(--moddin-accent, #7aa2f7);
  border-radius: 6px;
  padding: 0.4rem 0.9rem;
  cursor: pointer;
}

.ct-secondary:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
</style>
