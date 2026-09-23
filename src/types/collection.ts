/**
 * Shared types for the in-app "Mod Collections" workflow.
 *
 * A collection is a named, ordered list of capabilities that get
 * installed together with rollback support. Mirrors the public
 * surface of `crate::collection` and `crate::collection_runner` in
 * src-tauri/.
 */

export type CollectionOrigin = 'builtIn' | 'local' | 'community'

export type CollectionStatus = 'available' | 'planned'

export type CollectionCategory = 'vr' | 'graphics' | 'qol' | 'system'

export interface CollectionEntry {
  id: string
  required: boolean
  note?: string | null
  config?: Record<string, unknown> | null
}

export interface CollectionSpec {
  id: string
  displayName: string
  description?: string
  category: CollectionCategory
  status: CollectionStatus
  targetGame?: string | null
  requiredEngines?: string[]
  capabilities: CollectionEntry[]
  safetyNotes?: string[]
  origin: CollectionOrigin
}

export interface CollectionSummary {
  id: string
  displayName: string
  category: CollectionCategory
  status: CollectionStatus
  origin: CollectionOrigin
  targetGame: string | null
  capabilityCount: number
  requiredCount: number
  description: string
}

export type StepStatus = 'pending' | 'running' | 'completed' | 'skipped' | 'failed'

export type SessionStatus =
  | 'running'
  | 'paused'
  | 'continuing'
  | 'completed'
  | 'aborting'
  | 'closed'

export interface CompletedStep {
  capabilityId: string
  status: StepStatus
  error: string | null
  transactionId: string | null
}

export interface CollectionSessionView {
  id: string
  collectionId: string
  gameId: string
  gameName: string
  status: SessionStatus
  totalSteps: number
  nextIndex: number
  steps: CompletedStep[]
  lastError: string | null
  failedCapability: string | null
}

export interface CollectionInstallRequest {
  collectionId: string
  gameId: string
  gameName: string
  installDir: string
  executableDir: string
  config?: Record<string, unknown>
}

export interface CollectionInstallResult {
  ok: boolean
  paused: boolean
  session: CollectionSessionView
  error: string | null
}

export type ResumeDecision = 'continue' | 'abort'

export interface CollectionResumeResult {
  ok: boolean
  paused: boolean
  session: CollectionSessionView
  error: string | null
}

export interface CollectionAbortResult {
  ok: boolean
  session: CollectionSessionView
  rolledBack: string[]
  errors: string[]
}

export interface CollectionSpecSummary {
  id: string
  displayName: string
  category: CollectionCategory
  status: CollectionStatus
  targetGame: string | null
  capabilityCount: number
  requiredCount: number
}

export interface CollectionValidationResult {
  ok: boolean
  errors: string[]
  spec: CollectionSpecSummary | null
}

export interface CollectionSaveResult {
  ok: boolean
  id: string
  path: string
  overwrote: boolean
  errors: string[]
}
