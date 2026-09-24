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
 *
 * Frontend resilience: `build_author_prompt` is not yet implemented in
 * the Rust side on this branch (the capability_authoring crate is
 * scoped for a future PR). When the Rust command is missing we fall
 * back to a JavaScript prompt builder so the dialog still works. The
 * user never sees a raw "Command … not found" error.
 */

function isCommandMissing(error: unknown): boolean {
  if (!error) return false
  const message = typeof error === 'string' ? error : (error as { message?: string })?.message ?? String(error)
  return /command .* not found|unknown command|no such command|cannot find .* command|unregistered command/i.test(
    message,
  )
}

function buildPromptFallback(context: AuthorPromptContext): string {
  const game = context.gameName?.trim() || 'this game'
  const intent = context.intent?.trim() || '(describe what you want in one sentence)'
  const verbosity = context.verbosity === 'basic'
    ? 'Use plain language with the technical names in parentheses. Spell out each step. The user is not a Moddin developer.'
    : 'Be concise. The user already understands the Moddin flow. Skip onboarding explanations.'

  switch (context.mode) {
    case 'recommend':
      return [
        '# Task',
        `Look at the user's catalog of Moddin capabilities and the supported game "${game}".`,
        'Recommend up to 4 capabilities that together give the best safe setup for this player.',
        '',
        '# Player intent',
        intent,
        '',
        '# Output format',
        'Return a Moddin recommendations YAML block — the format the Moddin app validates with',
        '`community_recommendations_validate`. Do NOT invent module ids; only reference ids that exist',
        'in the catalog the user is using (ask if you need the catalog).',
        '',
        '# Tone',
        verbosity,
      ].join('\n')

    case 'diagnose':
      return [
        '# Task',
        `A user just hit this error in Moddin Desktop while working with "${game}":`,
        '',
        '```',
        intent,
        '```',
        '',
        '# What I want from you',
        '1. Translate the error into plain language (no jargon).',
        '2. List the 2-3 most likely root causes.',
        '3. For each, give the next thing the user should try.',
        '',
        '# Tone',
        verbosity,
      ].join('\n')

    case 'improve':
      return [
        '# Task',
        `The user installed a Moddin capability (id: ${context.capabilityId ?? '(unknown)'}) for "${game}".`,
        'They want to improve it. Read their intent below, draft an updated YAML, and explain the diff.',
        '',
        '# Player intent',
        intent,
        '',
        '# Output format',
        'Return the updated Moddin capability YAML in a fenced block.',
        '',
        '# Tone',
        verbosity,
      ].join('\n')

    case 'author':
    default:
      return [
        '# Task',
        `Author a brand-new Moddin capability for the game "${game}".`,
        'The capability must be safe, transactional, and follow the Moddin capability schema.',
        '',
        '# Player intent',
        intent,
        '',
        '# Output format',
        'Return the Moddin capability YAML in a fenced ```yaml``` block. Keep the file under 200 lines.',
        'Use one of the existing builtin steps; do not invent new step kinds.',
        '',
        '# Tone',
        verbosity,
      ].join('\n')
  }
}

export async function buildAuthorPrompt(
  context: AuthorPromptContext,
): Promise<string> {
  try {
    return await invoke<string>('build_author_prompt', { context })
  } catch (error) {
    if (isCommandMissing(error)) {
      // Rust side does not yet ship capability_authoring — fall back to
      // a JS template. The user sees the same prompt text either way.
      return buildPromptFallback(context)
    }
    throw error
  }
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
