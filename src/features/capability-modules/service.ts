import { invokeDebug as invoke } from '../../debug'
import type { CapabilitySpec, CapabilitySummary, InstallResult, ResolvedConfig } from '../../types/capability'
import type { TransactionRecord } from '../../types/transaction'
import type {
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
