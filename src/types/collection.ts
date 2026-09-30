/**
 * Shared types for the in-app "Mod Collections" workflow.
 *
 * A collection is a named, ordered list of capabilities that get
 * installed together. The catalog half of the shape — what a collection
 * is, and the `preset` that describes what installing it would do —
 * belongs to `src/features/collection/service.ts`, which is the only
 * thing that talks to the backend. It is re-exported here so a caller
 * has one import for the whole concept, and so there is exactly one
 * definition of `CollectionSummary` in the app.
 *
 * The install *session* is a frontend concept. The branch this came from
 * drove a backend `CollectionSession` through `collection_install` /
 * `collection_resume` / `collection_abort`; no such Rust module exists
 * in this build, and adding one would be a second install path. The run
 * is therefore a frontend loop over the ordinary capability install
 * command, which means there is no backend session id to resume against
 * and nothing here mirrors a Tauri payload.
 */

export type { CollectionPreset, CollectionSummary } from '../features/collection/service'

export type CollectionCategory = 'vr' | 'graphics' | 'qol' | 'system'

export type CollectionStepStatus =
  | 'pending'
  | 'running'
  | 'completed'
  | 'failed'
  | 'reverted'

export type CollectionPhase =
  | 'idle'
  | 'installing'
  | 'paused'
  | 'completed'
  | 'reverting'
  | 'reverted'
  | 'error'

/** One capability in the plan, as the dialog's ordered step list shows it. */
export interface CollectionInstallStep {
  capabilityId: string
  displayName: string
  status: CollectionStepStatus
  /**
   * Transaction the capability install recorded. Kept so a revert can
   * show which changes it undid without re-reading the whole list.
   */
  transactionId: string | null
  error: string | null
}

export interface CollectionSession {
  collectionId: string
  displayName: string
  gameId: string
  gameName: string
  totalSteps: number
  /** Index of the step currently running; `totalSteps` when finished. */
  nextIndex: number
  steps: CollectionInstallStep[]
  lastError: string | null
  failedCapability: string | null
}

/** What a revert managed to undo, and what it could not. */
export interface CollectionRollbackReport {
  reverted: string[]
  errors: string[]
}
