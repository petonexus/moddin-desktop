export type ModuleVerificationStatus = 'unknown' | 'ready' | 'installed' | 'attention'

export interface ModuleVerificationCheck {
  label: string
  passed: boolean
  detail?: string
}

export interface ModuleVerification {
  status: ModuleVerificationStatus
  summary: string
  checks: ModuleVerificationCheck[]
  checkedAt: number
  activeTransactionId: string | null
}
