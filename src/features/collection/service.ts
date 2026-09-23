import { invokeDebug as invoke } from '../../debug'
import type {
  CollectionAbortResult,
  CollectionInstallRequest,
  CollectionInstallResult,
  CollectionResumeResult,
  CollectionSaveResult,
  CollectionSpec,
  CollectionSpecSummary,
  CollectionSummary,
  CollectionValidationResult,
  ResumeDecision,
} from '../../types/collection'
import type { InstallResult, ResolvedConfig } from '../../types/capability'

/**
 * Thin Tauri bindings for the Mod Collections workflow.
 * The schema and the install state machine live in the Rust backend
 * (`crate::collection` + `crate::collection_runner`); the frontend
 * never re-implements them.
 */

export function listCollections(): Promise<CollectionSummary[]> {
  return invoke<CollectionSummary[]>('collection_list')
}

export function getCollection(id: string): Promise<CollectionSpec> {
  return invoke<CollectionSpec>('collection_get', { id })
}

export function installCollection(
  request: CollectionInstallRequest,
): Promise<CollectionInstallResult> {
  return invoke<CollectionInstallResult>('collection_install', { request })
}

export function resumeCollection(
  sessionId: string,
  decision: ResumeDecision,
): Promise<CollectionResumeResult> {
  return invoke<CollectionResumeResult>('collection_resume', {
    sessionId,
    decision,
  })
}

export function abortCollection(
  sessionId: string,
): Promise<CollectionAbortResult> {
  return invoke<CollectionAbortResult>('collection_abort', { sessionId })
}

export function validateCollectionYaml(
  yaml: string,
): Promise<CollectionValidationResult> {
  return invoke<CollectionValidationResult>('collection_validate_yaml', { yaml })
}

export function saveCollectionYaml(
  yaml: string,
  overwrite: boolean,
): Promise<CollectionSaveResult> {
  return invoke<CollectionSaveResult>('collection_save_yaml', { yaml, overwrite })
}

/**
 * Install a single capability by id. Used by the AI recommender flow
 * for items that aren't bundled in a collection. Mirrors the request
 * shape the Rust `capability_install` Tauri command expects.
 */
export interface InstallCapabilityRequest {
  capabilityId: string
  gameId: string
  gameName: string
  installDir: string
  executableDir: string
  config: ResolvedConfig
}

export function installCapability(
  request: InstallCapabilityRequest,
): Promise<InstallResult> {
  return invoke<InstallResult>('capability_install', { request })
}

export type { CollectionSpecSummary }
export type { InstallResult }
