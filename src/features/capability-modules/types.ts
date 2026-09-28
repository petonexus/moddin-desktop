import type { CapabilitySpec, CapabilitySummary, ResolvedConfig } from '../../types/capability'
import type { TransactionRecord } from '../../types/transaction'

/**
 * One evaluated check from `capability_evaluate`. Mirrors the fields the
 * UI needs from Rust `crate::module::CheckOutcome` (serde camelCase):
 * `id` is optional, `label`/`passed` always present, `detail` optional.
 */
export interface CapabilityCheckOutcome {
  id?: string
  label: string
  passed: boolean
  detail?: string
  category?: string
  severity?: string
}

/** Mirrors Rust `crate::module::VerificationReport` (serde camelCase). */
export interface CapabilityVerificationReport {
  status: string
  summary: string
  checks: CapabilityCheckOutcome[]
  gameRunning: boolean
  installed: boolean
  installedVersion?: string | null
}

export type CapabilityConfigValue = string | number | boolean | string[]

export interface CapabilityInstallParams {
  capabilityId: string
  gameId: string
  gameName: string
  installDir: string
  executableDir: string
  config: ResolvedConfig
}

export interface CapabilityUninstallParams {
  capabilityId: string
  gameId: string
  installDir: string
}

export interface CapabilityEvaluateParams {
  capabilityId: string
  executableDir: string
  config: ResolvedConfig
}

/** Per-card mutable state, keyed by capability id. */
export interface CapabilityCardState {
  busy: boolean
  verifyBusy: boolean
  error: string | null
  /** Which action produced `error` — the view picks the i18n key from it. */
  errorKind: 'install' | 'action' | 'validation' | null
  spec: CapabilitySpec | null
  specLoading: boolean
  verification: CapabilityVerificationReport | null
  configValues: Record<string, CapabilityConfigValue>
}

export interface UseCapabilityModulesOptions {
  gameId: () => string | null
  gameName: () => string | null
  installDir: () => string | null
  executableDir: () => string | null
  engine: () => string | null
  excludeIds: () => string[]
  transactions: () => TransactionRecord[]
  onChanged: () => void | Promise<void>
}

export type { CapabilitySpec, CapabilitySummary, ResolvedConfig, TransactionRecord }
