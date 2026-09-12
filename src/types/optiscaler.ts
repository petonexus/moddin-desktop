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
}

export interface ProxyConflict {
  name: string
  path: string
  sizeBytes: number
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
  changes: string[]
  warnings: string[]
}
