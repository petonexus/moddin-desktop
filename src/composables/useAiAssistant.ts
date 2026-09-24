import { computed, ref, watch } from 'vue'
import type {
  AuthorPreviewResult,
  AuthorPromptContext,
  AuthorPromptMode,
  AuthorPromptVerbosity,
  AuthorSaveResult,
  AuthorSpecSummary,
  AuthorValidationResult,
  Recommendation,
} from '../types/ai-assistant'
import {
  buildAuthorPrompt,
  previewCapabilityPlan,
  saveCapabilityYaml,
  validateCapabilityYaml,
  validateRecommendationsYaml,
  listCapabilitiesForAssistant,
} from '../features/ai-assistant/service'
import {
  installCollection,
  listCollections,
} from '../features/collection/service'
import { installCommunityCapability as installCapability } from '../features/community/service'
import { readLocalValue, writeLocalValue } from '../services/storage'
import type { CatalogCapability, CatalogCollection } from '../types/ai-assistant'
import type { CapabilitySummary } from '../types/capability'

/**
 * Singleton state for the "ask AI to author a Moddin capability"
 * dialog. Multiple trigger points in the UI (floating dock, community
 * panel, module cards, error banners) all drive the same dialog
 * instance by calling `aiAssistant.openFor(...)`.
 *
 * All Tauri commands are routed through `features/ai-assistant/service`
 * to keep the architecture boundary intact.
 */
const VERBOSITY_STORAGE_KEY = 'moddin-ai-verbosity'

function loadStoredVerbosity(): AuthorPromptVerbosity {
  const stored = readLocalValue(VERBOSITY_STORAGE_KEY)
  return stored === 'advanced' ? 'advanced' : 'basic'
}

const open = ref(false)
const step = ref<'prompt' | 'paste' | 'preview' | 'review' | 'installing' | 'saved'>('prompt')
const mode = ref<AuthorPromptMode>('author')
const verbosity = ref<AuthorPromptVerbosity>(loadStoredVerbosity())
const context = ref<AuthorPromptContext>({ mode: 'author', verbosity: verbosity.value })

const intent = ref('')
const yamlInput = ref('')
const promptText = ref('')

const validation = ref<AuthorValidationResult | null>(null)
const preview = ref<AuthorPreviewResult | null>(null)
const saveResult = ref<AuthorSaveResult | null>(null)
const promptCopied = ref(false)
const error = ref<string | null>(null)
const busy = ref(false)

// Recommender-mode specific state. Only meaningful when
// `mode.value === 'recommend'`; cleared otherwise.
const recommendations = ref<Recommendation[]>([])
const selectedRecommendations = ref<Set<string>>(new Set())
const recommendationInstallStatus = ref<Record<string, 'pending' | 'running' | 'done' | 'failed'>>({})

// Persist the user's verbosity choice so the next dialog open honors it.
watch(verbosity, (next) => {
  writeLocalValue(VERBOSITY_STORAGE_KEY, next)
})

function reset() {
  step.value = 'prompt'
  intent.value = ''
  yamlInput.value = ''
  promptText.value = ''
  validation.value = null
  preview.value = null
  saveResult.value = null
  promptCopied.value = false
  error.value = null
  busy.value = false
  recommendations.value = []
  selectedRecommendations.value = new Set()
  recommendationInstallStatus.value = {}
}

async function openFor(next: AuthorPromptContext) {
  reset()
  mode.value = next.mode
  // Carry the user's chosen verbosity into the request, but let the
  // caller override it explicitly (e.g. an "advanced" trigger deep-link).
  verbosity.value = next.verbosity ?? verbosity.value
  context.value = { ...next, verbosity: verbosity.value }
  if (next.intent) intent.value = next.intent
  open.value = true
  await regeneratePrompt()
}

function setVerbosity(next: AuthorPromptVerbosity) {
  if (verbosity.value === next) return
  verbosity.value = next
  context.value = { ...context.value, verbosity: next }
  // Re-generate the prompt so the user immediately sees the new tone.
  void regeneratePrompt()
}

function setMode(next: AuthorPromptMode) {
  if (mode.value === next) return
  mode.value = next
  context.value = { ...context.value, mode: next }
  // Reset to the first step so the user lands on the prompt screen for
  // the new mode (instead of staying on a stale preview/review).
  step.value = 'prompt'
  void regeneratePrompt()
}

async function regeneratePrompt() {
  busy.value = true
  error.value = null
  try {
    const next: AuthorPromptContext = {
      ...context.value,
      verbosity: verbosity.value,
      intent: intent.value.trim() || null,
    }
    context.value = next
    promptText.value = await buildAuthorPrompt(next)
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    busy.value = false
  }
}

async function copyPromptToClipboard(): Promise<boolean> {
  if (!promptText.value) return false
  try {
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      await navigator.clipboard.writeText(promptText.value)
    } else {
      const fallback = document.createElement('textarea')
      fallback.value = promptText.value
      fallback.style.position = 'fixed'
      fallback.style.opacity = '0'
      document.body.appendChild(fallback)
      fallback.select()
      document.execCommand('copy')
      document.body.removeChild(fallback)
    }
    promptCopied.value = true
    window.setTimeout(() => {
      promptCopied.value = false
    }, 2500)
    return true
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
    return false
  }
}

function extractYamlFence(input: string): string {
  const trimmed = input.trim()
  const fenceMatch = trimmed.match(/```(?:yaml|yml)?\s*\n([\s\S]*?)```/)
  if (fenceMatch) return fenceMatch[1].trim()
  return trimmed
}

async function validateYaml(): Promise<boolean> {
  busy.value = true
  error.value = null
  try {
    const candidate = extractYamlFence(yamlInput.value)
    if (mode.value === 'recommend') {
      const result = await validateRecommendationsYaml(candidate)
      if (result.ok) {
        recommendations.value = result.recommendations
        // Pre-select everything by default — the user can deselect
        // before installing.
        selectedRecommendations.value = new Set(
          result.recommendations.map((r) => `${r.type}:${r.id}`),
        )
        step.value = 'review'
      } else {
        recommendations.value = []
        selectedRecommendations.value = new Set()
        error.value = result.errors.join('\n')
      }
      return result.ok
    }
    const result = await validateCapabilityYaml(candidate)
    validation.value = result
    if (result.ok) {
      preview.value = await previewCapabilityPlan(candidate)
      step.value = 'preview'
    } else {
      preview.value = null
    }
    return result.ok
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
    validation.value = null
    preview.value = null
    return false
  } finally {
    busy.value = false
  }
}

function toggleRecommendation(key: string) {
  const next = new Set(selectedRecommendations.value)
  if (next.has(key)) next.delete(key)
  else next.add(key)
  selectedRecommendations.value = next
}

interface RecommendOpenContext {
  gameId: string | null
  gameName: string | null
  intent: string
  capabilityId?: string | null
}

function synthesizeCapabilityDescription(summary: CapabilitySummary): string {
  const cat = summary.category || 'qol'
  const origin = summary.origin === 'builtIn' ? 'integrada' : summary.origin === 'community' ? 'comunidade' : 'local'
  return `Capacidade da categoria ${cat}, origem ${origin}.`
}

function synthesizeCollectionDescription(
  summary: { id: string; displayName: string; description: string; targetGame: string | null; capabilityCount: number; requiredCount: number },
): string {
  return summary.description || `Pacote com ${summary.capabilityCount} capacidades (${summary.requiredCount} obrigatórias).`
}

/**
 * Open the AI dialog pre-configured for the recommend workflow.
 *
 * Fetches the live catalog (built-in + local capabilities, built-in +
 * local collections) and threads it into the prompt context so the AI
 * can pick concrete items. Used by the proactive game-banner and the
 * per-module "Por que essa?" button.
 */
async function openAiAssistantRecommendations(ctx: RecommendOpenContext) {
  busy.value = true
  error.value = null
  try {
    const [collections, capabilities] = await Promise.all([
      listCollections(),
      listCapabilitiesForAssistant(),
    ])

    const collectionCatalog: CatalogCollection[] = collections
      .filter((c) => c.targetGame === null || c.targetGame === ctx.gameId)
      .map((c) => ({
        id: c.id,
        displayName: c.displayName,
        category: c.category,
        description: synthesizeCollectionDescription(c),
        targetGame: c.targetGame,
        capabilityCount: c.capabilityCount,
      }))

    const capabilityCatalog: CatalogCapability[] = capabilities
      .map((c) => ({
        id: c.id,
        displayName: c.displayName,
        category: c.category,
        description: synthesizeCapabilityDescription(c),
        targetGame: null,
      }))

    await openFor({
      mode: 'recommend',
      verbosity: verbosity.value,
      gameId: ctx.gameId,
      gameName: ctx.gameName,
      capabilityId: ctx.capabilityId ?? null,
      intent: ctx.intent,
      capabilityCatalog,
      collectionCatalog,
    })
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
    // Even if the catalog fetch failed, fall back to opening the
    // dialog so the user isn't locked out — the prompt handles an
    // empty catalog gracefully.
    await openFor({
      mode: 'recommend',
      verbosity: verbosity.value,
      gameId: ctx.gameId,
      gameName: ctx.gameName,
      capabilityId: ctx.capabilityId ?? null,
      intent: ctx.intent,
    })
  } finally {
    busy.value = false
  }
}

interface InstallRecommendationContext {
  gameId: string
  gameName: string
  installDir: string
  executableDir: string
}

async function installSelectedRecommendations(ctx: InstallRecommendationContext) {
  const selected = recommendations.value.filter((r) =>
    selectedRecommendations.value.has(`${r.type}:${r.id}`),
  )
  if (selected.length === 0) return
  busy.value = true
  error.value = null
  // Snapshot the install status BEFORE flipping the step so the
  // template can render the pending placeholders right away.
  const status: Record<string, 'pending' | 'running' | 'done' | 'failed'> = {}
  for (const rec of selected) {
    status[`${rec.type}:${rec.id}`] = 'pending'
  }
  recommendationInstallStatus.value = status
  step.value = 'installing'

  for (const rec of selected) {
    const key = `${rec.type}:${rec.id}`
    recommendationInstallStatus.value = { ...recommendationInstallStatus.value, [key]: 'running' }
    try {
      if (rec.type === 'collection') {
        await installCollection({
          collectionId: rec.id,
          gameId: ctx.gameId,
          gameName: ctx.gameName,
          installDir: ctx.installDir,
          executableDir: ctx.executableDir,
        })
      } else {
        // For capabilities we go through the standard install path.
        // The capability_install command requires installDir/executableDir
        // and a config; for the recommend flow we let the user fill
        // those later (today we skip and surface an info message).
        // Implementation detail: this currently does a best-effort
        // install; if the capability needs config, the install fails
        // gracefully and the user can finish it from the main UI.
        await installCapability({
          capabilityId: rec.id,
          gameId: ctx.gameId,
          gameName: ctx.gameName,
          installDir: ctx.installDir,
          executableDir: ctx.executableDir,
          config: { values: {} },
          acceptUnsigned: true,
        })
      }
      recommendationInstallStatus.value = {
        ...recommendationInstallStatus.value,
        [key]: 'done',
      }
    } catch (err) {
      recommendationInstallStatus.value = {
        ...recommendationInstallStatus.value,
        [key]: 'failed',
      }
      // We don't bail out — try the next recommendation and surface
      // the error at the end.
      const msg = err instanceof Error ? err.message : String(err)
      error.value = error.value
        ? `${error.value}\n${rec.id}: ${msg}`
        : `${rec.id}: ${msg}`
    }
  }
  busy.value = false
  step.value = 'saved'
}

async function save(): Promise<boolean> {
  busy.value = true
  error.value = null
  try {
    const candidate = extractYamlFence(yamlInput.value)
    const result = await saveCapabilityYaml(candidate, false)
    saveResult.value = result
    if (result.ok) {
      step.value = 'saved'
      return true
    }
    error.value = result.errors.join('\n')
    return false
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
    return false
  } finally {
    busy.value = false
  }
}

function close() {
  open.value = false
  reset()
}

const specSummary = computed<AuthorSpecSummary | null>(() => {
  if (validation.value?.ok && validation.value.spec) {
    return validation.value.spec
  }
  return null
})

/**
 * Hook used by the dialog component itself. Returns the singleton
 * state and actions — there is exactly one dialog, mounted by
 * `GlobalTools`, so all triggers operate on this shared instance.
 */
export function useAiAssistant() {
  return {
    // state
    open,
    step,
    mode,
    verbosity,
    context,
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
    // actions
    openFor,
    openAiAssistantRecommendations,
    setMode,
    setVerbosity,
    regeneratePrompt,
    copyPromptToClipboard,
    validateYaml,
    save,
    close,
    toggleRecommendation,
    installSelectedRecommendations,
  }
}

/**
 * Trigger-side helper for callers that don't render the dialog but
 * want to open it from a button. Returns the openFor action plus the
 * higher-level `openAiAssistantRecommendations` which fetches the
 * catalog before opening.
 */
export function useAiAssistantTrigger() {
  return { openFor, openAiAssistantRecommendations }
}
