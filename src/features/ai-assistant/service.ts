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
 * workflow. Validation, preview and save are executed by the Rust
 * backend (`crate::capability_authoring`) against the same
 * `CapabilitySpec` schema the runner uses, so an AI-drafted YAML is
 * checked exactly like a shipped one.
 *
 * Frontend resilience: `build_author_prompt` is intentionally not
 * implemented in Rust (the prompt template iterates faster in
 * TypeScript). When the Rust command is missing we fall back to a
 * JavaScript prompt builder so the dialog still works. The user never
 * sees a raw "Command … not found" error.
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
        `Look at the user's catalog of Moddin mods and the supported game "${game}".`,
        'Recommend up to 4 mods that together give the best safe setup for this player.',
        '',
        '# Player intent',
        intent,
        '',
        '# Output format',
        'Return a Moddin recommendations YAML block — the format the Moddin app validates with',
        '`community_recommendations_validate`. Do NOT invent mod ids; only reference ids that exist',
        'in the catalog the user is using (ask if you need the catalog).',
        '',
        '# Tone',
        verbosity,
      ].join('\n')

    case 'diagnose': {
      // The trigger carries the raw error in `errorMessage`; `intent`
      // stays empty for diagnose opens. Fall back to whatever text we
      // have so the pasted prompt never shows an empty code block.
      const errorText = context.errorMessage?.trim() || intent
      return [
        '# Task',
        `A user just hit this error in Moddin Desktop while working with "${game}":`,
        '',
        '```',
        errorText,
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
    }

    case 'improve':
      return [
        '# Task',
        `The user installed a Moddin mod for "${game}" (internal id in the catalog file: ${context.capabilityId ?? 'unknown'}).`,
        '(In Moddin YAML files each mod is declared under a `modules:` list — that is what you will rewrite.)',
        'Read their intent below, draft the updated YAML, and explain what changed.',
        '',
        '# Player intent',
        intent,
        '',
        '# Output format',
        'Return the updated YAML for that mod in a fenced ```yaml``` block.',
        '',
        '# Tone',
        verbosity,
      ].join('\n')

    case 'author':
    default:
      return [
        '# Task',
        `Author a brand-new Moddin mod for the game "${game}".`,
        '(In Moddin YAML files each mod is declared under a `modules:` list with id, name, description, category, status, config and steps.)',
        'The mod must be safe, transactional, and follow the Moddin schema.',
        '',
        '# Player intent',
        intent,
        '',
        '# Output format',
        'Return the YAML for the new mod in a fenced ```yaml``` block. Keep the file under 200 lines.',
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
 * onboarding card. Goes through `open_web_url` (any HTTPS host) — the
 * release-oriented `open_external_url` only allows github.com links.
 */
export function openAiAssistantLink(url: string): Promise<void> {
  return invoke<void>('open_web_url', { url })
}

/**
 * List every capability Moddin knows about. Used by the recommender
 * to ground the AI in the live catalog before opening the dialog.
 */
export function listCapabilitiesForAssistant(): Promise<CapabilitySummary[]> {
  return invoke<CapabilitySummary[]>('capability_list')
}

/* --------------------------------------------------------------------------
 * Agent mode — drive an AI CLI already installed on the PC (Codex,
 * Claude Code, Cursor Agent) headlessly, instead of asking the user to
 * copy the prompt to a web AI and paste YAML back.
 * ------------------------------------------------------------------------ */

/** Which AI CLI Moddin should drive. Mirrors the Rust `AgentKind` enum. */
export type AgentKind = 'codex' | 'claudeCode' | 'cursorAgent'

/** Discovery snapshot for one AI CLI. Mirrors Rust `AgentCliInfo`. */
export interface AgentCliInfo {
  agent: AgentKind
  available: boolean
  path: string | null
  detail: string | null
}

export type AgentRunStatus = 'ok' | 'noYaml' | 'failed'

/** Outcome of one headless agent run. Mirrors Rust `AgentRunResult`. */
export interface AgentRunResult {
  agent: AgentKind
  status: AgentRunStatus
  output: string
  yaml: string | null
  durationMs: number
}

/**
 * Instructions appended to the generated prompt when it is handed to an
 * installed AI CLI. The agent has the Moddin MCP server available, so it
 * can ground itself in the real catalog and check its own YAML before
 * replying — and it must NOT save anything (the app saves after the user
 * reviews). Diagnose tasks legitimately answer in plain text instead of
 * YAML, which the backend reports as `noYaml`.
 */
export const AGENT_SUFFIX = `

---
You have the Moddin MCP server available (tools: list_supported_games, get_game_info, get_capability_template, list_capabilities, validate_capability_yaml, preview_capability_plan). Use them to ground your answer in the real catalog and to check your work. Do NOT call save_capability_yaml — the Moddin app saves after the user reviews. Finish your reply with the final result as a single fenced \`\`\`yaml block (\`\`\`yaml ...\`\`\`). If the task is a diagnosis (no YAML expected), just answer in plain language.`

/** Detect which AI CLIs are installed on this machine. */
export function listAgentClis(): Promise<AgentCliInfo[]> {
  return invoke<AgentCliInfo[]>('list_agent_clis')
}

/**
 * Run one prompt through the given CLI (headless, up to 240 s). The
 * backend serializes runs — a second call while one is active errors.
 */
export function runAiAgentPrompt(
  agent: AgentKind,
  prompt: string,
): Promise<AgentRunResult> {
  return invoke<AgentRunResult>('run_ai_agent_prompt', {
    request: { agent, prompt },
  })
}
