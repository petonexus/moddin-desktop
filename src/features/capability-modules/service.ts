import { invokeDebug as invoke } from '../../debug'
import type {
  CapabilityCheckOutcome,
  CapabilitySpec,
  CapabilitySummary,
  InstallResult,
  ResolvedConfig,
} from '../../types/capability'
import type { TransactionRecord } from '../../types/transaction'
import type {
  CapabilityCompatibilityParams,
  CapabilityEvaluateParams,
  CapabilityInstallParams,
  CapabilityUninstallParams,
  CapabilityVerificationReport,
} from './types'

export function listCapabilities() {
  return invoke<CapabilitySummary[]>('capability_list')
}

/** Full recipe for one capability (config schema, safety notes, checks). */
export function getCapabilitySpec(capabilityId: string) {
  return invoke<CapabilitySpec>('capability_get', { request: { capabilityId } })
}

/**
 * Probe only the spec's `compatibility` block. Resolves to `null` when
 * the spec supports every game build — cheaper than `evaluateCapability`,
 * which also downloads archives for the `archive-sha256` check.
 */
export function getCapabilityCompatibility(params: CapabilityCompatibilityParams) {
  return invoke<CapabilityCheckOutcome | null>('capability_compatibility', { request: params })
}

export function installCapability(params: CapabilityInstallParams) {
  return invoke<InstallResult>('capability_install', { request: params })
}

export function uninstallCapability(params: CapabilityUninstallParams) {
  return invoke<TransactionRecord>('capability_uninstall', { request: params })
}

export function evaluateCapability(params: CapabilityEvaluateParams) {
  return invoke<CapabilityVerificationReport>('capability_evaluate', { request: params })
}

export type { ResolvedConfig }
