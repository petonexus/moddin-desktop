export interface OptiScalerRequest {
  gameId: string
  gameName: string
  installDir: string
  executable: string
  version: string
  downloadUrl: string
  sha256: string
  proxyCandidates: string[]
  safetyNotes: string[]
  allowReplaceUnknown?: boolean
  resolution?: ProxyResolution
}

export type ProxyResolution = 'use_next_free' | 'replace_with_backup' | 'blocked'

export interface ProxyConflict {
  name: string
  path: string
  sizeBytes: number
  managedByModdin?: boolean
  heldBy?: string
}

export interface OptiScalerPreview {
  canApply: boolean
  gameRunning: boolean
  executableExists: boolean
  executablePath: string
  executableDirectory: string
  selectedProxy: string | null
  installed: boolean
  installedVersion: string | null
  currentProxy: string | null
  manualInstallDetected: boolean
  conflicts: ProxyConflict[]
  resolution?: ProxyResolution
  changes: string[]
  warnings: string[]
}
