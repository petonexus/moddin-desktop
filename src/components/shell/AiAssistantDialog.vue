<script setup lang="ts">
import { computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../ui/AppIcon.vue'
import BaseDialog from '../ui/BaseDialog.vue'
import { useAiAssistant } from '../../composables/useAiAssistant'
import { openAiAssistantLink } from '../../features/ai-assistant/service'
import type { AgentKind } from '../../features/ai-assistant/service'
import type { AuthorPromptMode, Recommendation } from '../../types/ai-assistant'

const { t } = useI18n()
const {
  open,
  step,
  mode,
  verbosity,
  intent,
  yamlInput,
  promptText,
  preview,
  saveResult,
  promptCopied,
  error,
  busy,
  specSummary,
  recommendations,
  selectedRecommendations,
  recommendationInstallStatus,
  agentClis,
  agentBusy,
  agentRun,
  agentElapsed,
  setMode,
  setVerbosity,
  regeneratePrompt,
  copyPromptToClipboard,
  validateYaml,
  save,
  close,
  toggleRecommendation,
  installSelectedRecommendations,
  ensureAgentClis,
  runWithAgent,
  dismissAgentRun,
} = useAiAssistant()

const modeMeta = computed(() => {
  switch (mode.value) {
    case 'recommend':
      return { key: 'aiAssistantModeRecommend', icon: 'community' as const }
    case 'diagnose':
      return { key: 'aiAssistantModeDiagnose', icon: 'alert' as const }
    case 'improve':
      return { key: 'aiAssistantModeImprove', icon: 'refresh' as const }
    case 'author':
    default:
      return { key: 'aiAssistantModeAuthor', icon: 'ai' as const }
  }
})

const modeChips: { mode: AuthorPromptMode; key: string; icon: 'community' | 'alert' | 'refresh' | 'ai' }[] = [
  { mode: 'recommend', key: 'aiAssistantChipRecommend', icon: 'community' },
  { mode: 'diagnose', key: 'aiAssistantChipDiagnose', icon: 'alert' },
  { mode: 'improve', key: 'aiAssistantChipImprove', icon: 'refresh' },
  { mode: 'author', key: 'aiAssistantChipAuthor', icon: 'ai' },
]

const verbosityLabel = computed(() =>
  verbosity.value === 'basic'
    ? t('aiAssistantVerbosityBasic')
    : t('aiAssistantVerbosityAdvanced'),
)

// Local fallback prompt (Rust command missing) is detected by the
// "# Task" marker with no inline error.
const fallbackNotice = computed(
  () => !error.value && promptText.value.startsWith('# Task'),
)

// Agent mode: detect installed AI CLIs (Codex / Claude Code / Cursor
// Agent) whenever the dialog opens. Runs once per session.
watch(open, (isOpen) => {
  if (isOpen) void ensureAgentClis()
})

const availableAgents = computed(() => agentClis.value.filter((cli) => cli.available))

const agentDisplayNames: Record<AgentKind, string> = {
  codex: 'Codex',
  claudeCode: 'Claude Code',
  cursorAgent: 'Cursor',
}

const needsOverwrite = computed(
  () => Boolean(error.value) && Boolean(saveResult.value?.exists),
)

function recKey(rec: Recommendation): string {
  return `${rec.type}:${rec.id}`
}

const selectedCount = computed(
  () => recommendations.value.filter((r) => selectedRecommendations.value.has(recKey(r))).length,
)

const installingRows = computed(() =>
  recommendations.value.filter((r) => selectedRecommendations.value.has(recKey(r))),
)

async function onCopyAndOpen() {
  await copyPromptToClipboard()
  await openAiAssistantLink('https://chatgpt.com/')
}

async function openAi(url: string) {
  await copyPromptToClipboard()
  await openAiAssistantLink(url)
}

function goToPaste() {
  step.value = 'paste'
}

function goToPrompt() {
  step.value = 'prompt'
}
</script>

<template>
  <Teleport to="body">
    <BaseDialog
      v-if="open"
      size="lg"
      :eyebrow="t('aiAssistantEyebrow')"
      :title="t('aiAssistantTitle')"
      :description="t(modeMeta.key)"
      @close="close"
    >
      <div class="ai-steps">
        <!-- ============ STEP: prompt (all modes) ============ -->
        <template v-if="step === 'prompt'">
          <button
            type="button"
            class="ai-cta"
            :class="{ 'is-loading': busy }"
            :disabled="busy"
            @click="regeneratePrompt"
          >
            <AppIcon :name="modeMeta.icon" :size="24" />
            <span class="ai-cta-text">
              <strong>{{ t('aiAssistantPrimaryCta') }}</strong>
              <small>{{ t('aiAssistantPrimaryCtaHint') }}</small>
            </span>
          </button>

          <div class="ai-chips" role="tablist" :aria-label="t('aiAssistantModePicker')">
            <button
              v-for="chip in modeChips"
              :key="chip.mode"
              type="button"
              role="tab"
              :aria-selected="mode === chip.mode"
              :class="['ai-chip', { 'is-active': mode === chip.mode }]"
              @click="setMode(chip.mode)"
            >
              <AppIcon :name="chip.icon" :size="13" />
              {{ t(chip.key) }}
            </button>
          </div>

          <!-- Agent mode: hand the prompt straight to an installed AI CLI. -->
          <div v-if="availableAgents.length" class="ai-agent-panel">
            <div class="ai-agent-head">
              <AppIcon name="ai" :size="16" />
              <strong>{{ t('aiAgentPanelTitle') }}</strong>
            </div>
            <div class="ai-agent-actions">
              <button
                v-for="cli in availableAgents"
                :key="cli.agent"
                type="button"
                class="btn btn-primary btn-sm"
                :class="{ 'is-loading': agentBusy }"
                :disabled="agentBusy || busy"
                @click="runWithAgent(cli.agent)"
              >
                {{ t('aiAgentRun', { agent: agentDisplayNames[cli.agent] }) }}
              </button>
            </div>
            <p v-if="agentBusy" class="ai-agent-status">
              <span class="spinner" aria-hidden="true" />
              {{ t('aiAgentRunning', { seconds: agentElapsed }) }}
            </p>
            <div v-if="agentRun" class="ai-agent-output">
              <div class="ai-agent-output-head">
                <strong>{{ t('aiAgentOutputTitle') }}</strong>
                <button class="btn btn-ghost btn-sm" type="button" @click="dismissAgentRun">
                  {{ t('aiAgentDismiss') }}
                </button>
              </div>
              <p v-if="agentRun.status === 'failed'" class="ai-meta">{{ t('aiAgentFailed') }}</p>
              <textarea class="textarea ai-prompt-output" rows="8" readonly :value="agentRun.output" />
            </div>
          </div>

          <label class="field">
            <span>{{ t('aiAssistantIntentLabel') }}</span>
            <textarea
              v-model="intent"
              class="textarea"
              rows="3"
              :placeholder="t('aiAssistantIntentPlaceholder')"
            />
            <small>{{ t('aiAssistantIntentHint') }}</small>
          </label>

          <details class="disclosure ai-disclosure">
            <summary>{{ t('aiAssistantAdvancedTitle') }}</summary>
            <div class="ai-disclosure-body">
              <span class="ai-meta-label">{{ t('aiAssistantVerbosityLabel') }}</span>
              <div class="ai-chips" role="tablist">
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
              <small class="ai-meta">{{ t('aiAssistantAdvancedHint', { value: verbosityLabel }) }}</small>
            </div>
          </details>

          <details v-if="verbosity === 'basic'" class="disclosure ai-disclosure">
            <summary>{{ t('aiAssistantBasicOnboardingTitle') }}</summary>
            <div class="ai-disclosure-body">
              <ol class="step-list">
                <li>{{ t('aiAssistantBasicOnboardingStep1') }}</li>
                <li>{{ t('aiAssistantBasicOnboardingStep2') }}</li>
                <li>{{ t('aiAssistantBasicOnboardingStep3') }}</li>
                <li>{{ t('aiAssistantBasicOnboardingStep4') }}</li>
              </ol>
              <span class="ai-meta-label">{{ t('aiAssistantBasicAiListLabel') }}</span>
              <div class="ai-links">
                <button class="btn btn-sm" type="button" @click="openAi('https://chatgpt.com/')">
                  <AppIcon name="external" :size="13" /> {{ t('aiAssistantBasicAiChatgpt') }}
                </button>
                <button class="btn btn-sm" type="button" @click="openAi('https://claude.ai/')">
                  <AppIcon name="external" :size="13" /> {{ t('aiAssistantBasicAiClaude') }}
                </button>
                <button class="btn btn-sm" type="button" @click="openAi('https://gemini.google.com/')">
                  <AppIcon name="external" :size="13" /> {{ t('aiAssistantBasicAiGemini') }}
                </button>
              </div>
              <small class="ai-meta">{{ t('aiAssistantPromptHintBasic') }}</small>
            </div>
          </details>

          <label class="field">
            <span>{{ t('aiAssistantPromptLabel') }}</span>
            <textarea
              v-model="promptText"
              class="textarea ai-prompt-output"
              rows="8"
              readonly
              :placeholder="t('aiAssistantPromptPlaceholder')"
            />
            <small v-if="fallbackNotice" class="ai-fallback">
              <AppIcon name="info" :size="12" /> {{ t('aiAssistantFallbackNotice') }}
            </small>
            <small v-if="mode === 'diagnose'" class="ai-meta">{{ t('aiAssistantPromptHintBasic') }}</small>
          </label>

          <div v-if="error" class="callout callout-danger" role="alert">
            <AppIcon name="alert" :size="16" />
            <p>{{ error }}</p>
          </div>
        </template>

        <!-- ============ STEP: paste ============ -->
        <template v-else-if="step === 'paste'">
          <p class="ai-step-heading">{{ t('aiAssistantStepPaste') }}</p>
          <label class="field">
            <span>{{ t('aiAssistantPasteLabel') }}</span>
            <textarea
              v-model="yamlInput"
              class="textarea ai-yaml-input"
              rows="10"
              :placeholder="t('aiAssistantPastePlaceholder')"
            />
          </label>
          <div v-if="error" class="callout callout-danger" role="alert">
            <strong>{{ t('aiAssistantValidationFailed') }}</strong>
            <p>{{ error }}</p>
          </div>
        </template>

        <!-- ============ STEP: preview (author / improve) ============ -->
        <template v-else-if="step === 'preview'">
          <p class="ai-step-heading">{{ t('aiAssistantStepReview') }}</p>

          <div v-if="specSummary" class="fact-grid">
            <div class="fact">
              <span>{{ t('aiAssistantStatInstallSteps') }}</span>
              <strong>{{ specSummary.installSteps }}</strong>
            </div>
            <div class="fact">
              <span>{{ t('aiAssistantStatUninstallSteps') }}</span>
              <strong>{{ specSummary.uninstallSteps }}</strong>
            </div>
            <div class="fact">
              <span>{{ t('aiAssistantStatVerifyChecks') }}</span>
              <strong>{{ specSummary.verifyChecks }}</strong>
            </div>
            <div class="fact">
              <span>{{ t('aiAssistantStatConfigFields') }}</span>
              <strong>{{ specSummary.configFields.length }}</strong>
            </div>
          </div>

          <p v-if="specSummary && specSummary.configFields.length" class="ai-meta">
            {{ t('aiAssistantConfigFieldsLabel') }}
            <span class="ai-mono">{{ specSummary.configFields.join(', ') }}</span>
          </p>

          <div v-if="specSummary && specSummary.safetyNotes.length" class="callout callout-warning">
            <AppIcon name="shield" :size="16" />
            <div>
              <strong>{{ t('aiAssistantSafetyNotes') }}</strong>
              <ul class="note-list">
                <li v-for="(note, index) in specSummary.safetyNotes" :key="index">{{ note }}</li>
              </ul>
            </div>
          </div>

          <div v-if="preview" class="dialog-section">
            <h3>{{ t('aiAssistantPreviewPlan') }}</h3>
            <pre class="ai-pre">{{ preview.plan }}</pre>
          </div>

          <div v-if="needsOverwrite" class="callout callout-danger">
            <AppIcon name="alert" :size="16" />
            <div>
              <strong>{{ t('aiAssistantOverwritePrompt') }}</strong>
              <p>
                <button
                  class="btn btn-danger btn-sm"
                  type="button"
                  :class="{ 'is-loading': busy }"
                  :disabled="busy"
                  @click="save(true)"
                >
                  {{ t('aiAssistantOverwrite') }}
                </button>
              </p>
            </div>
          </div>
          <div v-else-if="error" class="callout callout-danger" role="alert">
            <AppIcon name="alert" :size="16" />
            <p>{{ error }}</p>
          </div>
        </template>

        <!-- ============ STEP: review (recommend) ============ -->
        <template v-else-if="step === 'review'">
          <p class="ai-step-heading">{{ t('aiAssistantStepReviewRecommendations') }}</p>
          <p class="ai-meta">{{ t('aiAssistantRecommendationsFound', { count: recommendations.length }) }}</p>

          <div class="ai-recs">
            <label
              v-for="rec in recommendations"
              :key="recKey(rec)"
              class="ai-rec"
              :class="{ 'is-disabled': rec.type === 'collection' }"
            >
              <input
                type="checkbox"
                :checked="selectedRecommendations.has(recKey(rec))"
                :disabled="rec.type === 'collection'"
                @change="toggleRecommendation(recKey(rec))"
              />
              <span class="ai-rec-body">
                <span class="ai-rec-top">
                  <strong>{{ rec.id }}</strong>
                  <span class="badge badge-accent badge-plain">
                    {{ t('aiAssistantRecommendationConfidence', { value: `${Math.round(rec.confidence * 100)}%` }) }}
                  </span>
                </span>
                <small>{{ rec.reason }}</small>
                <small v-if="rec.type === 'collection'" class="ai-meta">
                  {{ t('aiAssistantCollectionUnavailable') }}
                </small>
              </span>
            </label>
          </div>
        </template>

        <!-- ============ STEP: installing (recommend) ============ -->
        <template v-else-if="step === 'installing'">
          <p class="ai-step-heading">{{ t('aiAssistantStepInstalling') }}</p>
          <p class="ai-meta">{{ t('aiAssistantInstallingRecommendations') }}</p>
          <ul class="ai-install-list">
            <li v-for="rec in installingRows" :key="recKey(rec)">
              <span v-if="recommendationInstallStatus[recKey(rec)] === 'running'" class="spinner" aria-hidden="true" />
              <AppIcon
                v-else-if="recommendationInstallStatus[recKey(rec)] === 'done'"
                name="check"
                :size="14"
                class="ai-status-done"
              />
              <AppIcon
                v-else-if="recommendationInstallStatus[recKey(rec)] === 'failed'"
                name="alert"
                :size="14"
                class="ai-status-failed"
              />
              <span v-else class="ai-status-pending" aria-hidden="true" />
              <span class="ai-rec-id">{{ rec.id }}</span>
            </li>
          </ul>
        </template>

        <!-- ============ STEP: saved ============ -->
        <template v-else-if="step === 'saved'">
          <div class="empty-state ai-saved">
            <AppIcon name="check" :size="28" class="ai-status-done" />
            <strong v-if="saveResult">{{ t('aiAssistantSavedHeading', { id: saveResult.id }) }}</strong>
            <strong v-else>{{ t('aiAssistantInstallDoneHeading') }}</strong>
            <span v-if="saveResult">{{ t('aiAssistantSavedBody') }}</span>
            <span v-else>{{ t('aiAssistantInstallDoneBody') }}</span>
            <small v-if="saveResult?.overwrote" class="ai-meta">{{ t('aiAssistantOverwrote') }}</small>
          </div>
          <div v-if="error" class="callout callout-danger" role="alert">
            <AppIcon name="alert" :size="16" />
            <p>{{ error }}</p>
          </div>
        </template>
      </div>

      <template #footer>
        <!-- prompt footer -->
        <template v-if="step === 'prompt'">
          <button class="btn btn-ghost" type="button" :disabled="!promptText" @click="copyPromptToClipboard">
            <AppIcon v-if="!promptCopied" name="check" :size="14" />
            {{ promptCopied ? t('aiAssistantCopied') : t('aiAssistantCopyPrompt') }}
          </button>
          <button class="btn" type="button" :disabled="!promptText" @click="onCopyAndOpen">
            <AppIcon name="external" :size="14" /> {{ t('aiAssistantCopyAndOpen') }}
          </button>
          <button v-if="mode !== 'diagnose'" class="btn btn-primary" type="button" @click="goToPaste">
            {{ t('aiAssistantContinueToPaste') }}
          </button>
        </template>

        <!-- paste footer -->
        <template v-else-if="step === 'paste'">
          <button class="btn btn-ghost" type="button" @click="goToPrompt">
            <AppIcon name="undo" :size="14" /> {{ t('aiAssistantBack') }}
          </button>
          <button
            class="btn btn-primary"
            :class="{ 'is-loading': busy }"
            type="button"
            :disabled="busy || !yamlInput.trim()"
            @click="validateYaml"
          >
            {{ busy ? t('aiAssistantValidating') : t('aiAssistantValidate') }}
          </button>
        </template>

        <!-- preview footer -->
        <template v-else-if="step === 'preview'">
          <button class="btn btn-ghost" type="button" @click="goToPaste">
            <AppIcon name="undo" :size="14" /> {{ t('aiAssistantBack') }}
          </button>
          <button
            class="btn btn-primary"
            :class="{ 'is-loading': busy }"
            type="button"
            :disabled="busy"
            @click="save()"
          >
            {{ busy ? t('aiAssistantSaving') : t('aiAssistantSave') }}
          </button>
        </template>

        <!-- review footer -->
        <template v-else-if="step === 'review'">
          <button class="btn btn-ghost" type="button" @click="goToPaste">
            <AppIcon name="undo" :size="14" /> {{ t('aiAssistantBack') }}
          </button>
          <button
            class="btn btn-primary"
            type="button"
            :disabled="selectedCount === 0"
            @click="installSelectedRecommendations"
          >
            <AppIcon name="play" :size="14" />
            {{ t('aiAssistantInstallSelected', { count: selectedCount }) }}
          </button>
        </template>

        <!-- saved footer -->
        <template v-else-if="step === 'saved'">
          <button class="btn btn-primary" type="button" @click="close">
            {{ t('aiAssistantDone') }}
          </button>
        </template>
      </template>
    </BaseDialog>
  </Teleport>
</template>

<style scoped>
.ai-steps { display: grid; gap: var(--moddin-space-4); }

.ai-cta {
  display: flex;
  align-items: center;
  gap: var(--moddin-space-3);
  width: 100%;
  text-align: left;
  padding: var(--moddin-space-4);
  border: 1px solid var(--moddin-accent-ring);
  border-radius: var(--moddin-radius-md);
  color: var(--moddin-accent-text);
  background: var(--moddin-accent-soft);
  font: inherit;
  cursor: pointer;
  transition: background var(--moddin-fast) var(--moddin-ease), border-color var(--moddin-fast) var(--moddin-ease);
}
.ai-cta:hover:not(:disabled) { border-color: var(--moddin-accent); }
.ai-cta:disabled { opacity: 0.6; cursor: not-allowed; }
.ai-cta-text { display: grid; gap: 2px; min-width: 0; }
.ai-cta-text strong { color: var(--moddin-text); font-size: var(--moddin-text-base); }
.ai-cta-text small { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }

.ai-agent-panel {
  display: grid;
  gap: var(--moddin-space-3);
  padding: var(--moddin-space-4);
  border: 1px solid var(--moddin-accent-ring);
  border-radius: var(--moddin-radius-md);
  background: var(--moddin-accent-soft);
}
.ai-agent-head { display: flex; align-items: center; gap: var(--moddin-space-2); }
.ai-agent-head .app-icon { color: var(--moddin-accent-text); }
.ai-agent-head strong { font-size: var(--moddin-text-base); }
.ai-agent-actions { display: flex; flex-wrap: wrap; gap: var(--moddin-space-2); }
.ai-agent-status {
  display: flex;
  align-items: center;
  gap: var(--moddin-space-2);
  margin: 0;
  color: var(--moddin-text-muted);
  font-size: var(--moddin-text-sm);
}
.ai-agent-output { display: grid; gap: var(--moddin-space-2); }
.ai-agent-output-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--moddin-space-2);
}
.ai-agent-output-head strong { color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }

.ai-chips { display: flex; flex-wrap: wrap; gap: var(--moddin-space-2); }
.ai-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 5px 11px;
  border: 1px solid var(--moddin-line-soft);
  border-radius: var(--moddin-radius-pill);
  color: var(--moddin-text-muted);
  background: var(--moddin-surface-2);
  font-size: var(--moddin-text-sm);
  font-weight: 600;
  cursor: pointer;
  transition: background var(--moddin-fast) var(--moddin-ease), border-color var(--moddin-fast) var(--moddin-ease), color var(--moddin-fast) var(--moddin-ease);
}
.ai-chip:hover { color: var(--moddin-text); background: var(--moddin-surface-3); }
.ai-chip.is-active {
  border-color: var(--moddin-accent-ring);
  color: var(--moddin-accent-text);
  background: var(--moddin-accent-soft);
}

.ai-disclosure > summary {
  color: var(--moddin-text-muted);
  font-size: var(--moddin-text-sm);
  font-weight: 600;
}
.ai-disclosure-body { display: grid; gap: var(--moddin-space-2); margin-top: var(--moddin-space-3); }
.ai-meta-label { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); font-weight: 600; }
.ai-meta { color: var(--moddin-text-muted); font-size: var(--moddin-text-xs); }
.ai-mono { font-family: var(--moddin-mono); }

.ai-links { display: flex; flex-wrap: wrap; gap: var(--moddin-space-2); }

.ai-prompt-output,
.ai-yaml-input {
  font-family: var(--moddin-mono);
  font-size: var(--moddin-text-sm);
  line-height: 1.5;
}
.ai-yaml-input { min-height: 180px; }

.ai-fallback {
  display: inline-flex;
  align-items: flex-start;
  gap: 5px;
  color: var(--moddin-info);
  font-size: var(--moddin-text-xs);
}
.ai-fallback .app-icon { margin-top: 2px; }

.ai-step-heading { color: var(--moddin-text-soft); font-size: var(--moddin-text-base); font-weight: 700; }

.ai-pre {
  margin: 0;
  max-height: 220px;
  overflow: auto;
  padding: var(--moddin-space-3);
  border: 1px solid var(--moddin-line-soft);
  border-radius: var(--moddin-radius-md);
  color: var(--moddin-text-soft);
  background: var(--moddin-surface-sunken);
  font: 12px/1.5 var(--moddin-mono);
  white-space: pre-wrap;
  word-break: break-word;
}

.ai-recs { display: grid; gap: var(--moddin-space-2); }
.ai-rec {
  display: flex;
  align-items: flex-start;
  gap: var(--moddin-space-3);
  padding: var(--moddin-space-3) var(--moddin-space-4);
  border: 1px solid var(--moddin-line-soft);
  border-radius: var(--moddin-radius-md);
  background: var(--moddin-surface-2);
  cursor: pointer;
}
.ai-rec:hover { border-color: var(--moddin-line-strong); }
.ai-rec.is-disabled { opacity: 0.55; cursor: not-allowed; }
.ai-rec input { margin-top: 3px; accent-color: var(--moddin-accent); }
.ai-rec-body { display: grid; gap: 3px; min-width: 0; }
.ai-rec-top { display: flex; align-items: center; flex-wrap: wrap; gap: var(--moddin-space-2); }
.ai-rec-top strong { font-size: var(--moddin-text-md); overflow-wrap: anywhere; }
.ai-rec-body small { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }

.ai-install-list { display: grid; gap: var(--moddin-space-2); margin: 0; padding: 0; list-style: none; }
.ai-install-list li {
  display: flex;
  align-items: center;
  gap: var(--moddin-space-2);
  padding: var(--moddin-space-2) var(--moddin-space-3);
  border: 1px solid var(--moddin-line-soft);
  border-radius: var(--moddin-radius-md);
  background: var(--moddin-surface-2);
  font-size: var(--moddin-text-sm);
}
.ai-rec-id { font-family: var(--moddin-mono); overflow-wrap: anywhere; }
.ai-status-pending {
  width: 8px;
  height: 8px;
  flex: 0 0 auto;
  border-radius: 50%;
  background: var(--moddin-text-faint);
  opacity: 0.5;
}
.ai-status-done { color: var(--moddin-success); }
.ai-status-failed { color: var(--moddin-danger); }

.ai-saved .app-icon { margin-bottom: var(--moddin-space-1); }
</style>
