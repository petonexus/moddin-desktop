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
  close,
  toggleRecommendation,
  installSelectedRecommendations,
} = useAiAssistant()

const dialogElement = ref<HTMLElement | null>(null)

useDialogLifecycle(open)

const fallbackNotice = computed(() => {
  // Show a banner explaining the dialog is using the local prompt
  // template — only relevant when the Rust backend is missing the
  // build_author_prompt command. Detected by the prompt text starting
  // with "# Task" (our fallback marker) without an inline error.
  return !error.value && promptText.value.startsWith('# Task') && step.value === 'prompt'
})

function closeDialog() {
  open.value = false
}

function closeOnBackdrop(event: MouseEvent) {
  if (event.target === event.currentTarget) {
    closeDialog()
  }
}

const modeMeta = computed(() => {
  switch (mode.value) {
    case 'recommend':
      return { glyph: '🎯', key: 'aiAssistantModeRecommend' }
    case 'diagnose':
      return { glyph: '⚠️', key: 'aiAssistantModeDiagnose' }
    case 'improve':
      return { glyph: '🛠️', key: 'aiAssistantModeImprove' }
    case 'author':
    default:
      return { glyph: '✨', key: 'aiAssistantModeAuthor' }
  }
})

const verbosityLabel = computed(() =>
  verbosity.value === 'basic'
    ? t('aiAssistantVerbosityBasic')
    : t('aiAssistantVerbosityAdvanced'),
)

function pickMode(next: 'author' | 'recommend' | 'diagnose' | 'improve') {
  // setMode lives on the singleton — go through the public hook
  void next
  useAiAssistant().setMode(next)
}

function onCopyAndOpen() {
  copyPromptToClipboard()
  const url =
    locale.value.startsWith('pt')
      ? 'https://chatgpt.com/?model=auto'
      : locale.value.startsWith('es')
        ? 'https://chatgpt.com/?model=auto'
        : 'https://chatgpt.com/?model=auto'
  void openAiAssistantLink(url)
}
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
        <div class="ai-header-text">
          <small class="ai-eyebrow">{{ t('aiAssistantEyebrow') }}</small>
          <h2>{{ t('aiAssistantTitle') }}</h2>
          <p>{{ t(modeMeta.key) }}</p>
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

      <div class="ai-body">
        <!-- Primary CTA card: the dominant action for the current mode. -->
        <button
          type="button"
          class="ai-primary-cta"
          @click="regeneratePrompt"
          :disabled="busy"
        >
          <span class="ai-primary-glyph" aria-hidden="true">{{ modeMeta.glyph }}</span>
          <span class="ai-primary-text">
            <span class="ai-primary-title">{{ t('aiAssistantPrimaryCta') }}</span>
            <span class="ai-primary-sub">{{ t('aiAssistantPrimaryCtaHint') }}</span>
          </span>
        </button>

        <!-- Secondary chips: switch between modes without leaving the flow. -->
        <div class="ai-mode-chips" role="tablist" :aria-label="t('aiAssistantModePicker')">
          <button
            type="button"
            role="tab"
            :aria-selected="mode === 'recommend'"
            :class="['ai-chip', { 'is-active': mode === 'recommend' }]"
            @click="pickMode('recommend')"
          >
            <span aria-hidden="true">🎯</span> {{ t('aiAssistantChipRecommend') }}
          </button>
          <button
            type="button"
            role="tab"
            :aria-selected="mode === 'diagnose'"
            :class="['ai-chip', { 'is-active': mode === 'diagnose' }]"
            @click="pickMode('diagnose')"
          >
            <span aria-hidden="true">⚠️</span> {{ t('aiAssistantChipDiagnose') }}
          </button>
          <button
            type="button"
            role="tab"
            :aria-selected="mode === 'improve'"
            :class="['ai-chip', { 'is-active': mode === 'improve' }]"
            @click="pickMode('improve')"
          >
            <span aria-hidden="true">🛠️</span> {{ t('aiAssistantChipImprove') }}
          </button>
          <button
            type="button"
            role="tab"
            :aria-selected="mode === 'author'"
            :class="['ai-chip', { 'is-active': mode === 'author' }]"
            @click="pickMode('author')"
          >
            <span aria-hidden="true">✨</span> {{ t('aiAssistantChipAuthor') }}
          </button>
        </div>

        <hr class="ai-divider" aria-hidden="true" />

        <!-- Intent: the only required user input. Big, single-field. -->
        <label class="ai-field">
          <span class="ai-field-label">{{ t('aiAssistantIntentLabel') }}</span>
          <textarea
            v-model="intent"
            class="ai-textarea"
            rows="3"
            :placeholder="t('aiAssistantIntentPlaceholder')"
          />
          <small class="ai-field-hint">{{ t('aiAssistantIntentHint') }}</small>
        </label>

        <!-- Verbosity: collapsed by default. Users who want to change
             it can, but the default ("basic") works for 90% of cases. -->
        <details class="ai-advanced">
          <summary>{{ t('aiAssistantAdvancedTitle') }}</summary>
          <div class="ai-advanced-body">
            <span class="ai-field-label">{{ t('aiAssistantVerbosityLabel') }}</span>
            <div class="ai-verbosity-toggle" role="tablist">
              <button
                type="button"
                role="tab"
                :aria-selected="verbosity === 'basic'"
                :class="['ai-chip', { 'is-active': verbosity === 'basic' }]"
                @click="setVerbosity('basic')"
              >
                {{ t('aiAssistantVerbosityBasic') }}
              </button>
              <button
                type="button"
                role="tab"
                :aria-selected="verbosity === 'advanced'"
                :class="['ai-chip', { 'is-active': verbosity === 'advanced' }]"
                @click="setVerbosity('advanced')"
              >
                {{ t('aiAssistantVerbosityAdvanced') }}
              </button>
            </div>
            <small class="ai-field-hint">{{ t('aiAssistantAdvancedHint', { value: verbosityLabel }) }}</small>
          </div>
        </details>

        <hr class="ai-divider" aria-hidden="true" />

        <!-- Output: the prompt to paste into the AI. -->
        <label class="ai-field">
          <span class="ai-field-label">{{ t('aiAssistantPromptLabel') }}</span>
          <textarea
            v-model="promptText"
            class="ai-textarea ai-textarea-output"
            rows="8"
            readonly
            :placeholder="t('aiAssistantPromptPlaceholder')"
          />
          <small class="ai-field-hint" v-if="fallbackNotice">{{ t('aiAssistantFallbackNotice') }}</small>
        </label>

        <!-- Errors stay visible but small, attached to the output they
             affect. Never replaces the action buttons. -->
        <div v-if="error" class="ai-inline-error" role="alert">
          {{ error }}
        </div>
      </div>

      <footer class="ai-footer">
        <button class="btn btn-ghost" type="button" @click="copyPromptToClipboard" :disabled="!promptText">
          <span aria-hidden="true">📋</span> {{ t('aiAssistantCopyPrompt') }}
        </button>
        <button class="btn btn-primary" type="button" @click="onCopyAndOpen" :disabled="!promptText">
          <span aria-hidden="true">✨</span> {{ t('aiAssistantCopyAndOpen') }}
        </button>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.ai-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: grid;
  place-items: center;
  z-index: 200;
  padding: 16px;
}

.ai-panel {
  background: var(--moddin-surface, #15171c);
  color: var(--moddin-text, #e8ecf2);
  border-radius: 16px;
  border: 1px solid var(--moddin-line, rgba(255, 255, 255, 0.08));
  width: min(640px, 100%);
  max-height: min(820px, calc(100vh - 32px));
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 24px 60px rgba(0, 0, 0, 0.45);
}

.ai-header {
  padding: 20px 24px;
  border-bottom: 1px solid var(--moddin-line-soft, rgba(255, 255, 255, 0.06));
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.ai-header-text { display: grid; gap: 2px; min-width: 0; }
.ai-eyebrow {
  font-size: 11px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--moddin-accent, #7aa2f7);
  font-weight: 700;
}
.ai-header h2 { margin: 0; font-size: 18px; font-weight: 700; }
.ai-header p { margin: 0; color: var(--moddin-text-muted, #9aa3b2); font-size: 13px; }

.ai-icon-button {
  background: transparent;
  color: var(--moddin-text-muted, #9aa3b2);
  border: 1px solid transparent;
  width: 32px;
  height: 32px;
  border-radius: 8px;
  cursor: pointer;
  font-size: 18px;
  line-height: 1;
}
.ai-icon-button:hover { background: var(--moddin-surface-2, rgba(255, 255, 255, 0.04)); }

.ai-body {
  padding: 16px 24px;
  display: grid;
  gap: 16px;
  overflow-y: auto;
  flex: 1;
}

.ai-primary-cta {
  display: flex;
  align-items: center;
  gap: 14px;
  text-align: left;
  background: linear-gradient(135deg, rgba(122, 162, 247, 0.32), rgba(122, 162, 247, 0.14));
  border: 1px solid rgba(122, 162, 247, 0.7);
  color: inherit;
  border-radius: 12px;
  padding: 16px 20px;
  cursor: pointer;
  font: inherit;
  transition: background 120ms ease, transform 80ms ease;
}
.ai-primary-cta:hover:not(:disabled) {
  background: linear-gradient(135deg, rgba(122, 162, 247, 0.45), rgba(122, 162, 247, 0.22));
}
.ai-primary-cta:active:not(:disabled) { transform: translateY(1px); }
.ai-primary-cta:disabled { opacity: 0.5; cursor: not-allowed; }
.ai-primary-glyph { font-size: 28px; }
.ai-primary-text { display: grid; gap: 2px; min-width: 0; }
.ai-primary-title { font-size: 16px; font-weight: 700; }
.ai-primary-sub { font-size: 12px; color: var(--moddin-text-muted, #9aa3b2); }

.ai-mode-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.ai-chip {
  background: var(--moddin-surface-2, rgba(255, 255, 255, 0.04));
  color: var(--moddin-text-muted, #9aa3b2);
  border: 1px solid var(--moddin-line-soft, rgba(255, 255, 255, 0.06));
  border-radius: 999px;
  padding: 6px 12px;
  font-size: 12px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  transition: background 120ms, border-color 120ms, color 120ms;
}
.ai-chip:hover { background: var(--moddin-surface-3, rgba(255, 255, 255, 0.08)); color: inherit; }
.ai-chip.is-active {
  background: rgba(122, 162, 247, 0.18);
  border-color: rgba(122, 162, 247, 0.55);
  color: inherit;
}

.ai-divider {
  border: 0;
  height: 1px;
  background: var(--moddin-line-soft, rgba(255, 255, 255, 0.06));
  margin: 0;
}

.ai-field {
  display: grid;
  gap: 6px;
}
.ai-field-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--moddin-text-muted, #9aa3b2);
}
.ai-field-hint { font-size: 11px; color: var(--moddin-text-muted, #9aa3b2); }

.ai-textarea {
  width: 100%;
  background: var(--moddin-surface-2, rgba(255, 255, 255, 0.04));
  color: inherit;
  border: 1px solid var(--moddin-line-soft, rgba(255, 255, 255, 0.08));
  border-radius: 8px;
  padding: 10px 12px;
  font: 13px/1.5 ui-monospace, "Cascadia Mono", "JetBrains Mono", monospace;
  resize: vertical;
  min-height: 64px;
}
.ai-textarea:focus {
  outline: 2px solid rgba(122, 162, 247, 0.45);
  outline-offset: 1px;
}
.ai-textarea-output {
  background: var(--moddin-surface-3, rgba(0, 0, 0, 0.18));
}

.ai-advanced {
  border: 1px solid var(--moddin-line-soft, rgba(255, 255, 255, 0.06));
  border-radius: 8px;
  padding: 0 12px;
  background: var(--moddin-surface-2, rgba(255, 255, 255, 0.02));
}
.ai-advanced > summary {
  cursor: pointer;
  padding: 10px 0;
  font-size: 12px;
  font-weight: 600;
  color: var(--moddin-text-muted, #9aa3b2);
  list-style: none;
}
.ai-advanced > summary::marker,
.ai-advanced > summary::-webkit-details-marker { display: none; }
.ai-advanced[open] > summary { color: inherit; }
.ai-advanced-body { padding: 0 0 12px; display: grid; gap: 8px; }

.ai-inline-error {
  background: var(--moddin-danger-bg, rgba(255, 80, 80, 0.12));
  color: var(--moddin-danger, #ff8a8a);
  border: 1px solid var(--moddin-danger-line, rgba(255, 80, 80, 0.3));
  border-radius: 8px;
  padding: 8px 12px;
  font-size: 12px;
}

.ai-footer {
  padding: 12px 24px;
  border-top: 1px solid var(--moddin-line-soft, rgba(255, 255, 255, 0.06));
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  background: var(--moddin-surface, #15171c);
}
.ai-footer .btn { padding: 8px 14px; border-radius: 8px; font-size: 13px; cursor: pointer; }
.ai-footer .btn:disabled { opacity: 0.5; cursor: not-allowed; }
.ai-footer .btn-ghost {
  background: transparent;
  color: inherit;
  border: 1px solid var(--moddin-line-soft, rgba(255, 255, 255, 0.12));
}
.ai-footer .btn-primary {
  background: linear-gradient(135deg, #7aa2f7, #5b8def);
  color: #0c0e12;
  border: none;
  font-weight: 700;
}
</style>
