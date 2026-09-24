import { invokeDebug as invoke } from '../../debug'
import type {
  AuthorPreviewResult,
  AuthorPromptContext,
  AuthorSaveResult,
  AuthorValidationResult,
  RecommendationsValidationResult,
} from '../../types/ai-assistant'
import type { CapabilitySummary } from '../../types/capability'

/**
 * Thin Tauri bindings for the "ask AI to author a Moddin capability"
 * workflow. The schema / validation logic lives in the Rust backend
 * (`crate::capability_authoring`); the frontend never re-implements it.
 */

export function buildAuthorPrompt(
  context: AuthorPromptContext,
): Promise<string> {
  return invoke<string>('build_author_prompt', { context })
}

export function validateCapabilityYaml(
  yaml: string,
): Promise<AuthorValidationResult> {
  return invoke<AuthorValidationResult>('validate_capability_yaml', { yaml })
}

export function validateRecommendationsYaml(
  yaml: string,
): Promise<RecommendationsValidationResult> {
  return invoke<RecommendationsValidationResult>(
    'validate_recommendations_yaml',
    { yaml },
  )
}

export function previewCapabilityPlan(
  yaml: string,
): Promise<AuthorPreviewResult> {
  return invoke<AuthorPreviewResult>('preview_capability_plan', { yaml })
}

export function saveCapabilityYaml(
  yaml: string,
  overwrite: boolean,
): Promise<AuthorSaveResult> {
  return invoke<AuthorSaveResult>('save_capability_yaml', { yaml, overwrite })
}

/**
 * Open a HTTPS URL in the user's default browser. Used by the AI
 * dialog to deep-link to ChatGPT / Claude / Gemini from the basic
 * onboarding card.
 */
export function openAiAssistantLink(url: string): Promise<void> {
  return invoke<void>('open_external_url', { url })
}

/**
 * List every capability Moddin knows about. Used by the recommender
 * to ground the AI in the live catalog before opening the dialog.
 */
export function listCapabilitiesForAssistant(): Promise<CapabilitySummary[]> {
  return invoke<CapabilitySummary[]>('capability_list')
}
