export type CompatibilityStatus = 'unverified' | 'experimental' | 'proven' | 'risky' | 'not_working'

export interface CompatibilityReport {
  status: CompatibilityStatus
  note: string
  testedVersion: string
  updatedAt: number
}
