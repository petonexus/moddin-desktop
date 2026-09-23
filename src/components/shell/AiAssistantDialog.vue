<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDialogLifecycle } from '../../composables/useDialogLifecycle'
import { useAiAssistant } from '../../composables/useAiAssistant'
import { openAiAssistantLink } from '../../features/ai-assistant/service'

const { t, locale } = useI18n()
const {
  open,
  step,
  mode,
  verbosity,
  intent,
  yamlInput,
  promptText,
  validation,
  preview,
  saveResult,
  promptCopied,
  error,
  busy,
  specSummary,
  recommendations,
  selectedRecommendations,
  recommendationInstallStatus,
  setVerbosity,
  regeneratePrompt,
  copyPromptToClipboard,
  validateYaml,
  save,
  toggleRecommendation,
  installSelectedRecommendations,
} = useAiAssistant()
const dialogElement = ref<HTMLElement | null>(null)
const { closeDialog } = useDialogLifecycle(open)

function setMode(next: 'author' | 'improve' | 'diagnose' | 'recommend') {
  mode.value = next
  // When switching mode, drop any leftover validate/save state from
  // the previous mode so the dialog starts clean.
  validation.value = null
  preview.value = null
  saveResult.value = null
}

async function installRecommendations() {
  const gameId = readSelectedGameId() ?? ''
  const gameName = readSelectedGameName() ?? ''
  await installSelectedRecommendations({
    gameId,
    gameName,
    installDir: '',
    executableDir: '',
  })
}

function readSelectedGameId(): string | null {
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

function recommendationKey(type: string, id: string) {
  return `${type}:${id}`
}

function confidenceLabel(confidence: number): string {
  return `${Math.round(confidence * 100)}%`
}

interface AiRecommendation {
  id: 'chatgpt' | 'claude' | 'gemini'
  url: string
}

const aiRecommendations: AiRecommendation[] = [
  { id: 'chatgpt', url: 'https://chatgpt.com/' },
  { id: 'claude', url: 'https://claude.ai/' },
  { id: 'gemini', url: 'https://gemini.google.com/' },
]

async function openExternal(url: string) {
  // Defence in depth — only HTTPS links are ever passed in (the
  // `aiRecommendations` list is hard-coded), but we still validate
  // the scheme before handing it to the Rust command.
  if (!/^https:\/\//i.test(url)) return
  try {
    await openAiAssistantLink(url)
  } catch {
    if (typeof window !== 'undefined') window.open(url, '_blank', 'noopener,noreferrer')
  }
}

function closeOnBackdrop(event: MouseEvent) {
  if (event.target === event.currentTarget) {
    closeDialog()
  }
}

const modeLabel = computed(() => {
  if (step.value === 'preview' || step.value === 'saved') {
    return t('aiAssistantStepReview')
  }
  if (step.value === 'paste') {
    return t('aiAssistantStepPaste')
  }
  if (step.value === 'review') {
    return t('aiAssistantStepReviewRecommendations')
  }
  if (step.value === 'installing') {
    return t('aiAssistantStepInstalling')
  }
  if (mode.value === 'improve') return t('aiAssistantModeImprove')
  if (mode.value === 'diagnose') return t('aiAssistantModeDiagnose')
  if (mode.value === 'recommend') return t('aiAssistantModeRecommend')
  return t('aiAssistantModeAuthor')
})
</script>

<template>
  <div v-if="open" class="ai-backdrop" @click="closeOnBackdrop">
    <section
      ref="dialogElement"
      class="ai-panel"
      role="dialog"
      aria-modal="true"
      :aria-label="t('aiAssistantTitle')"
      tabindex="-1"
    >
      <header class="ai-header">
        <div>
          <small>AI</small>
          <h2>{{ t('aiAssistantTitle') }}</h2>
          <p>{{ modeLabel }}</p>
        </div>
        <button
          class="ai-icon-button"
          type="button"
          :aria-label="t('aiAssistantClose')"
          @click="closeDialog"
        >
          ×
        </button>
      </header>

      <!-- Mode tabs (author / improve / diagnose / recommend) -->
      <div class="ai-mode-tabs" role="tablist">
        <button
          type="button"
          role="tab"
          :class="{ active: mode === 'author' }"
          :aria-selected="mode === 'author'"
          @click="setMode('author')"
        >
          {{ t('aiAssistantModeAuthor') }}
        </button>
        <button
          type="button"
          role="tab"
          :class="{ active: mode === 'improve' }"
          :aria-selected="mode === 'improve'"
          @click="setMode('improve')"
        >
          {{ t('aiAssistantModeImprove') }}
        </button>
        <button
          type="button"
          role="tab"
          :class="{ active: mode === 'diagnose' }"
          :aria-selected="mode === 'diagnose'"
          @click="setMode('diagnose')"
        >
          {{ t('aiAssistantModeDiagnose') }}
        </button>
        <button
          type="button"
          role="tab"
          :class="{ active: mode === 'recommend' }"
          :aria-selected="mode === 'recommend'"
          @click="setMode('recommend')"
        >
          {{ t('aiAssistantModeRecommend') }}
        </button>
      </div>

      <!-- Verbosity toggle (basic = passo a passo, advanced = direto) -->
      <div class="ai-verbosity-toggle" role="tablist" :aria-label="t('aiAssistantVerbosityLabel')">
        <button
          type="button"
          role="tab"
          :aria-selected="verbosity === 'basic'"
          :class="{ active: verbosity === 'basic' }"
          @click="setVerbosity('basic')"
        >
          {{ t('aiAssistantVerbosityBasic') }}
        </button>
        <button
          type="button"
          role="tab"
          :aria-selected="verbosity === 'advanced'"
          :class="{ active: verbosity === 'advanced' }"
          @click="setVerbosity('advanced')"
        >
          {{ t('aiAssistantVerbosityAdvanced') }}
        </button>
      </div>

      <div v-if="error" class="ai-error" role="alert">
        {{ error }}
      </div>

      <!-- Step 1: prompt -->
      <div v-if="step === 'prompt'" class="ai-step">
        <!-- Basic-mode onboarding cards. Only shown the first time the
             dialog opens (intent is still empty). -->
        <details
          v-if="verbosity === 'basic' && !intent"
          class="ai-onboarding"
          open
        >
          <summary>{{ t('aiAssistantBasicOnboardingTitle') }}</summary>
          <ol class="ai-onboarding-steps">
            <li>{{ t('aiAssistantBasicOnboardingStep1') }}</li>
            <li>{{ t('aiAssistantBasicOnboardingStep2') }}</li>
            <li>{{ t('aiAssistantBasicOnboardingStep3') }}</li>
            <li>{{ t('aiAssistantBasicOnboardingStep4') }}</li>
          </ol>
          <div class="ai-ai-list">
            <p>{{ t('aiAssistantBasicAiListLabel') }}</p>
            <div class="ai-ai-buttons">
              <button
                v-for="rec in aiRecommendations"
                :key="rec.id"
                type="button"
                class="ai-ai-button"
                @click="openExternal(rec.url)"
              >
                {{ t(`aiAssistantBasicAi${rec.id.charAt(0).toUpperCase()}${rec.id.slice(1)}`) }}
                <span aria-hidden="true">↗</span>
              </button>
            </div>
          </div>
        </details>

        <label class="ai-field">
          <span>{{ t('aiAssistantIntentLabel') }}</span>
          <textarea
            v-model="intent"
            rows="4"
            :placeholder="t('aiAssistantIntentPlaceholder')"
            :disabled="busy"
          />
          <small>{{ t('aiAssistantIntentHint') }}</small>
        </label>

        <div class="ai-actions">
          <button
            class="ai-secondary"
            type="button"
            :disabled="busy"
            @click="regeneratePrompt()"
          >
            {{ t('aiAssistantRegenerate') }}
          </button>
          <button
            class="ai-secondary"
            type="button"
            :disabled="!promptText"
            @click="copyPromptToClipboard()"
          >
            {{ promptCopied
              ? t('aiAssistantCopied')
              : t('aiAssistantCopyPrompt') }}
          </button>
        </div>

        <label class="ai-field">
          <span>{{ t('aiAssistantPromptLabel') }}</span>
          <textarea
            v-model="promptText"
            class="ai-prompt-area"
            rows="14"
            readonly
          />
          <small v-if="verbosity === 'basic'">
            {{ t('aiAssistantPromptHintBasic') }}
          </small>
        </label>

        <div class="ai-actions ai-actions-right">
          <button class="ai-primary" type="button" @click="step = 'paste'">
            {{ t('aiAssistantContinueToPaste') }}
          </button>
        </div>
      </div>

      <!-- Step 2: paste YAML -->
      <div v-if="step === 'paste'" class="ai-step">
        <label class="ai-field">
          <span>{{ t('aiAssistantPasteLabel') }}</span>
          <textarea
            v-model="yamlInput"
            rows="16"
            :placeholder="t('aiAssistantPastePlaceholder')"
            :disabled="busy"
            class="ai-yaml-area"
          />
        </label>

        <div v-if="validation && !validation.ok" class="ai-error" role="alert">
          <strong>{{ t('aiAssistantValidationFailed') }}</strong>
          <ul>
            <li v-for="(line, index) in validation.errors" :key="index">{{ line }}</li>
          </ul>
        </div>

        <div class="ai-actions ai-actions-right">
          <button
            class="ai-secondary"
            type="button"
            :disabled="busy"
            @click="step = 'prompt'"
          >
            {{ t('aiAssistantBack') }}
          </button>
          <button
            class="ai-primary"
            type="button"
            :disabled="busy || !yamlInput.trim()"
            @click="validateYaml()"
          >
            {{ busy
              ? t('aiAssistantValidating')
              : t('aiAssistantValidate') }}
          </button>
        </div>
      </div>

      <!-- Step 3a: review recommendations (only in recommend mode) -->
      <div v-if="step === 'review'" class="ai-step">
        <p class="ai-recommendations-intro">
          {{ t('aiAssistantRecommendationsFound', { count: recommendations.length }) }}
        </p>
        <ul class="ai-recommendations-list">
          <li
            v-for="rec in recommendations"
            :key="recommendationKey(rec.type, rec.id)"
            class="ai-recommendation-row"
          >
            <label class="ai-recommendation-checkbox">
              <input
                type="checkbox"
                :checked="selectedRecommendations.has(recommendationKey(rec.type, rec.id))"
                @change="toggleRecommendation(recommendationKey(rec.type, rec.id))"
              />
              <span>
                <strong>{{ rec.id }}</strong>
                <span class="ai-recommendation-type">{{ rec.type }}</span>
                <span class="ai-recommendation-confidence">
                  {{ t('aiAssistantRecommendationConfidence', { value: confidenceLabel(rec.confidence) }) }}
                </span>
                <small>{{ rec.reason }}</small>
              </span>
            </label>
          </li>
        </ul>
        <div class="ai-actions ai-actions-right">
          <button
            class="ai-secondary"
            type="button"
            @click="step = 'paste'"
          >
            {{ t('aiAssistantBack') }}
          </button>
          <button
            class="ai-primary"
            type="button"
            :disabled="busy || selectedRecommendations.size === 0"
            @click="installRecommendations()"
          >
            {{ t('aiAssistantInstallSelected', { count: selectedRecommendations.size }) }}
          </button>
        </div>
      </div>

      <!-- Step 3b: installing recommendations (progress) -->
      <div v-if="step === 'installing'" class="ai-step">
        <p class="ai-recommendations-intro">
          {{ t('aiAssistantInstallingRecommendations') }}
        </p>
        <ul class="ai-recommendations-list">
          <li
            v-for="rec in recommendations"
            :key="recommendationKey(rec.type, rec.id)"
            class="ai-recommendation-row"
          >
            <span class="ai-recommendation-status">
              <template v-if="recommendationInstallStatus[recommendationKey(rec.type, rec.id)] === 'done'">✓</template>
              <template v-else-if="recommendationInstallStatus[recommendationKey(rec.type, rec.id)] === 'failed'">✕</template>
              <template v-else-if="recommendationInstallStatus[recommendationKey(rec.type, rec.id)] === 'running'">•</template>
              <template v-else>○</template>
            </span>
            <span>
              <strong>{{ rec.id }}</strong>
              <small>{{ rec.reason }}</small>
            </span>
          </li>
        </ul>
      </div>

      <!-- Step 3: preview / confirm save -->
      <div v-if="step === 'preview' && specSummary" class="ai-step">
        <div class="ai-summary">
          <h3>{{ specSummary.displayName }}</h3>
          <p class="ai-summary-meta">
            <code>{{ specSummary.id }}</code>
            <span>·</span>
            <span>{{ specSummary.category }}</span>
            <span>·</span>
            <span>{{ specSummary.status }}</span>
          </p>
          <ul class="ai-summary-stats">
            <li>
              <strong>{{ specSummary.installSteps }}</strong>
              <span>{{ t('aiAssistantStatInstallSteps') }}</span>
            </li>
            <li>
              <strong>{{ specSummary.uninstallSteps }}</strong>
              <span>{{ t('aiAssistantStatUninstallSteps') }}</span>
            </li>
            <li>
              <strong>{{ specSummary.verifyChecks }}</strong>
              <span>{{ t('aiAssistantStatVerifyChecks') }}</span>
            </li>
            <li>
              <strong>{{ specSummary.configFields.length }}</strong>
              <span>{{ t('aiAssistantStatConfigFields') }}</span>
            </li>
          </ul>
          <p v-if="specSummary.configFields.length > 0" class="ai-config-fields">
            <span>{{ t('aiAssistantConfigFieldsLabel') }}</span>
            <code v-for="field in specSummary.configFields" :key="field">
              {{ field }}
            </code>
          </p>
          <p v-if="specSummary.safetyNotes.length > 0" class="ai-notes">
            <strong>{{ t('aiAssistantSafetyNotes') }}</strong>
            <span v-for="(note, index) in specSummary.safetyNotes" :key="index">
              ⚠ {{ note }}
            </span>
          </p>
        </div>

        <details v-if="preview" class="ai-preview-plan">
          <summary>{{ t('aiAssistantPreviewPlan') }}</summary>
          <pre>{{ preview.plan }}</pre>
        </details>

        <div class="ai-actions ai-actions-right">
          <button
            class="ai-secondary"
            type="button"
            :disabled="busy"
            @click="step = 'paste'"
          >
            {{ t('aiAssistantBack') }}
          </button>
          <button
            class="ai-primary"
            type="button"
            :disabled="busy"
            @click="save()"
          >
            {{ busy
              ? t('aiAssistantSaving')
              : t('aiAssistantSave') }}
          </button>
        </div>
      </div>

      <!-- Step 4: saved -->
      <div v-if="step === 'saved' && saveResult" class="ai-step">
        <div class="ai-success" role="status">
          <strong>{{ t('aiAssistantSavedHeading', { id: saveResult.id }) }}</strong>
          <p>{{ t('aiAssistantSavedBody') }}</p>
          <code class="ai-saved-path">{{ saveResult.path }}</code>
          <p v-if="saveResult.overwrote" class="ai-overwrite-note">
            {{ t('aiAssistantOverwrote') }}
          </p>
        </div>
        <div class="ai-actions ai-actions-right">
          <button class="ai-primary" type="button" @click="closeDialog">
            {{ t('aiAssistantDone') }}
          </button>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.ai-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  z-index: 9100;
  display: flex;
  align-items: center;
  justify-content: center;
}

.ai-panel {
  width: min(760px, 96vw);
  max-height: 90vh;
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

.ai-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
}

.ai-header small {
  letter-spacing: 0.08em;
  color: var(--moddin-accent, #7aa2f7);
  font-size: 0.75rem;
  font-weight: 700;
}

.ai-header h2 {
  margin: 0.1rem 0 0.2rem;
  font-size: 1.1rem;
}

.ai-header p {
  margin: 0;
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.85rem;
}

.ai-icon-button {
  background: transparent;
  border: 1px solid var(--moddin-border, #3a4252);
  color: inherit;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  cursor: pointer;
  font-size: 1.1rem;
}

.ai-mode-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
  border-bottom: 1px solid var(--moddin-border, #3a4252);
}

.ai-mode-tabs button {
  background: transparent;
  border: 0;
  color: var(--moddin-muted, #8c93a3);
  padding: 0.45rem 0.85rem;
  font-size: 0.82rem;
  cursor: pointer;
  border-bottom: 2px solid transparent;
}

.ai-mode-tabs button.active {
  color: var(--moddin-accent, #7aa2f7);
  border-bottom-color: var(--moddin-accent, #7aa2f7);
  font-weight: 600;
}

.ai-verbosity-toggle {
  display: inline-flex;
  align-self: flex-start;
  border: 1px solid var(--moddin-border, #3a4252);
  border-radius: 999px;
  overflow: hidden;
  background: var(--moddin-surface-2, #1a1f29);
}

.ai-verbosity-toggle button {
  background: transparent;
  border: 0;
  color: var(--moddin-muted, #8c93a3);
  padding: 0.35rem 0.9rem;
  font-size: 0.8rem;
  cursor: pointer;
}

.ai-verbosity-toggle button.active {
  background: var(--moddin-accent, #7aa2f7);
  color: #0c0e15;
  font-weight: 600;
}

.ai-step {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.ai-onboarding {
  background: var(--moddin-surface-2, #1a1f29);
  border: 1px solid var(--moddin-border, #3a4252);
  border-radius: 6px;
  padding: 0.6rem 0.8rem;
  font-size: 0.85rem;
}

.ai-onboarding summary {
  cursor: pointer;
  font-weight: 600;
  color: var(--moddin-accent, #7aa2f7);
}

.ai-onboarding-steps {
  margin: 0.5rem 0;
  padding-left: 1.2rem;
  color: var(--moddin-muted, #8c93a3);
}

.ai-onboarding-steps li {
  margin-bottom: 0.25rem;
}

.ai-ai-list {
  margin-top: 0.6rem;
}

.ai-ai-list > p {
  margin: 0 0 0.4rem;
  font-size: 0.78rem;
  color: var(--moddin-muted, #8c93a3);
}

.ai-ai-buttons {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}

.ai-ai-button {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  background: transparent;
  border: 1px solid var(--moddin-border, #3a4252);
  color: var(--moddin-text, #e8ecf2);
  padding: 0.35rem 0.7rem;
  border-radius: 999px;
  font-size: 0.78rem;
  cursor: pointer;
}

.ai-ai-button:hover {
  border-color: var(--moddin-accent, #7aa2f7);
}

.ai-field {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  font-size: 0.85rem;
}

.ai-field > span {
  font-weight: 600;
  color: var(--moddin-muted, #8c93a3);
}

.ai-field small {
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.75rem;
}

.ai-field textarea {
  background: var(--moddin-surface-2, #1a1f29);
  color: inherit;
  border: 1px solid var(--moddin-border, #3a4252);
  border-radius: 6px;
  padding: 0.55rem 0.7rem;
  font-family: 'Cascadia Code', 'Consolas', monospace;
  font-size: 0.85rem;
  resize: vertical;
}

.ai-field textarea:disabled {
  opacity: 0.6;
}

.ai-prompt-area {
  min-height: 220px;
}

.ai-yaml-area {
  min-height: 260px;
}

.ai-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}

.ai-actions-right {
  justify-content: flex-end;
}

.ai-primary {
  background: var(--moddin-accent, #7aa2f7);
  color: #0c0e15;
  border: none;
  border-radius: 6px;
  padding: 0.45rem 1rem;
  font-weight: 600;
  cursor: pointer;
}

.ai-primary:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.ai-secondary {
  background: transparent;
  border: 1px solid var(--moddin-accent, #7aa2f7);
  color: var(--moddin-accent, #7aa2f7);
  border-radius: 6px;
  padding: 0.4rem 0.9rem;
  cursor: pointer;
}

.ai-secondary:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.ai-error {
  background: rgba(255, 90, 90, 0.12);
  border: 1px solid rgba(255, 90, 90, 0.4);
  color: #ff8c8c;
  padding: 0.5rem 0.7rem;
  border-radius: 6px;
  font-size: 0.85rem;
}

.ai-error ul {
  margin: 0.3rem 0 0;
  padding-left: 1.2rem;
}

.ai-success {
  background: rgba(40, 200, 120, 0.12);
  border: 1px solid rgba(40, 200, 120, 0.4);
  color: #2ecf86;
  padding: 0.7rem 0.9rem;
  border-radius: 6px;
  font-size: 0.9rem;
}

.ai-saved-path {
  display: block;
  margin-top: 0.4rem;
  word-break: break-all;
  background: rgba(0, 0, 0, 0.25);
  padding: 0.4rem 0.5rem;
  border-radius: 4px;
  color: inherit;
  font-size: 0.78rem;
}

.ai-overwrite-note {
  margin: 0.4rem 0 0;
  font-style: italic;
  color: var(--moddin-warning, #f0b432);
}

.ai-summary h3 {
  margin: 0;
  font-size: 1rem;
}

.ai-summary-meta {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  margin: 0.2rem 0 0.6rem;
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.85rem;
}

.ai-summary-meta code {
  background: var(--moddin-surface-2, #1a1f29);
  padding: 0.05rem 0.35rem;
  border-radius: 4px;
}

.ai-summary-stats {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.ai-summary-stats li {
  background: var(--moddin-surface-2, #1a1f29);
  border: 1px solid var(--moddin-border, #3a4252);
  border-radius: 6px;
  padding: 0.4rem 0.7rem;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.1rem;
  min-width: 80px;
}

.ai-summary-stats strong {
  font-size: 1.1rem;
}

.ai-summary-stats span {
  font-size: 0.72rem;
  color: var(--moddin-muted, #8c93a3);
  text-align: center;
}

.ai-config-fields,
.ai-notes {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
  align-items: center;
  font-size: 0.8rem;
  color: var(--moddin-muted, #8c93a3);
}

.ai-config-fields code {
  background: var(--moddin-surface-2, #1a1f29);
  padding: 0.05rem 0.35rem;
  border-radius: 4px;
  color: var(--moddin-text, #e8ecf2);
}

.ai-notes {
  flex-direction: column;
  align-items: flex-start;
}

.ai-preview-plan {
  background: var(--moddin-surface-2, #1a1f29);
  border: 1px solid var(--moddin-border, #3a4252);
  border-radius: 6px;
  padding: 0.5rem 0.75rem;
  font-size: 0.8rem;
}

.ai-preview-plan summary {
  cursor: pointer;
  font-weight: 600;
  color: var(--moddin-muted, #8c93a3);
}

.ai-preview-plan pre {
  margin: 0.5rem 0 0;
  white-space: pre-wrap;
  font-family: 'Cascadia Code', 'Consolas', monospace;
  font-size: 0.78rem;
}

.ai-recommendations-intro {
  margin: 0;
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.85rem;
}

.ai-recommendations-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  max-height: 360px;
  overflow-y: auto;
}

.ai-recommendation-row {
  background: var(--moddin-surface-2, #1a1f29);
  border: 1px solid var(--moddin-border, #3a4252);
  border-radius: 6px;
  padding: 0.5rem 0.7rem;
  font-size: 0.85rem;
  display: grid;
  grid-template-columns: 24px 1fr;
  gap: 0.5rem;
  align-items: start;
}

.ai-recommendation-checkbox {
  display: contents;
}

.ai-recommendation-checkbox > input {
  margin-top: 0.2rem;
}

.ai-recommendation-checkbox > span {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
}

.ai-recommendation-checkbox strong {
  font-family: 'Cascadia Code', 'Consolas', monospace;
}

.ai-recommendation-checkbox small {
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.78rem;
}

.ai-recommendation-type {
  display: inline-block;
  background: rgba(122, 162, 247, 0.15);
  color: var(--moddin-accent, #7aa2f7);
  font-size: 0.7rem;
  padding: 0.05rem 0.4rem;
  border-radius: 999px;
  margin-left: 0.4rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.ai-recommendation-confidence {
  display: inline-block;
  margin-left: 0.4rem;
  color: var(--moddin-muted, #8c93a3);
  font-size: 0.75rem;
}

.ai-recommendation-status {
  text-align: center;
  font-weight: 600;
}
</style>
