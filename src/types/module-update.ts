export type ModuleUpdateStatus = 'unknown' | 'current' | 'available' | 'unavailable' | 'error'

export interface ModuleUpdate {
  status: ModuleUpdateStatus
  currentVersion: string | null
  latestVersion: string | null
  releaseUrl: string | null
  checkedAt: number
  detail?: string
}
