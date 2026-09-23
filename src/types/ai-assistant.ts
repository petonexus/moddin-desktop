/**
 * Shared types for the in-app "ask AI to author / improve / diagnose a
 * Moddin capability" workflow.
 *
 * Mirrors the public surface of `crate::capability_authoring` in
 * src-tauri/. The Rust side enforces the schema; the frontend just
 * displays the result and feeds back what the AI returns.
 */

export type AuthorPromptMode = 'author' | 'improve' | 'diagnose' | 'recommend'

/**
 * Lightweight snapshot of one capability in the catalog. Used to
 * ground the AI recommender in the user's actual installable items.
 */
export interface CatalogCapability {
  id: string
  displayName: string
  category: string
  description: string
  targetGame?: string | null
}

/**
 * Same idea for collections. Mirrors `CatalogCollection` in Rust.
 */
export interface CatalogCollection {
  id: string
  displayName: string
  category: string
  description: string
  targetGame?: string | null
  capabilityCount: number
}

/**
 * A single recommendation the AI returned in `recommend` mode.
 * Mirrors the Rust `Recommendation` struct.
 */
export interface Recommendation {
  type: 'collection' | 'capability'
  id: string
  reason: string
  confidence: number
}

export interface RecommendationsValidationResult {
  ok: boolean
  errors: string[]
  recommendations: Recommendation[]
}

/**
 * `basic` walks the user through the whole flow in plain Portuguese
 * (default — the dialog opens here). `advanced` is the terse English
 * prompt aimed at developers / AI agents that already know what a
 * YAML schema is.
 */
export type AuthorPromptVerbosity = 'basic' | 'advanced'

export interface AuthorPromptContext {
  mode: AuthorPromptMode
  verbosity?: AuthorPromptVerbosity
  gameId?: string | null
  gameName?: string | null
  capabilityId?: string | null
  intent?: string | null
  errorMessage?: string | null
  errorSource?: string | null
  capabilityCatalog?: CatalogCapability[]
  collectionCatalog?: CatalogCollection[]
}

export interface AuthorSpecSummary {
  id: string
  displayName: string
  category: string
  status: string
  supportedEngines: string[]
  configFields: string[]
  installSteps: number
  uninstallSteps: number
  verifyChecks: number
  safetyNotes: string[]
}

export interface AuthorValidationResult {
  ok: boolean
  errors: string[]
  spec?: AuthorSpecSummary | null
}

export interface AuthorPreviewResult {
  ok: boolean
  plan: string
}

export interface AuthorSaveResult {
  ok: boolean
  id: string
  path: string
  overwrote: boolean
  errors: string[]
}
