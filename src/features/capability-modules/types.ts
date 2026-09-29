import type {
  CapabilityCheckOutcome,
  CapabilitySpec,
  CapabilitySummary,
  ResolvedConfig,
} from '../../types/capability'
import type { TransactionRecord } from '../../types/transaction'

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
  /** Install even when the spec's compatibility block rejects the game build. */
  force?: boolean
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

export interface CapabilityCompatibilityParams {
  capabilityId: string
  executableDir: string
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
  /**
   * Result of the spec's `compatibility` probe (`capability_compatibility`).
   * `null` when the spec does not constrain the game build, when no game
   * is selected, or before the probe ran. A non-null `passed: false`
   * blocks a plain install — the card offers a forced one instead.
   */
  compatibility: CapabilityCheckOutcome | null
  compatibilityBusy: boolean
  /**
   * Capabilities the backend auto-installed as dependencies of the last
   * install, in install order. Shown once so the user knows what came
   * along; cleared by the next install.
   */
  installedDependencies: string[]
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

export type {
  CapabilityCheckOutcome,
  CapabilitySpec,
  CapabilitySummary,
  ResolvedConfig,
  TransactionRecord,
}
