import type { TransactionRecord } from './transaction'

export interface OfxrRequest {
  gameId: string
  gameName: string
  installDir: string
  executable: string
  version: string
  implementationVersion: number
  downloadUrl: string
  sha256: string
  backend: string
  nvidiaPreset: string
  nvidiaInputScale: number
  nvidiaBidirectional: boolean
  forceReinstall?: boolean
  safetyNotes: string[]
}

export interface OfxrPreview {
  canApply: boolean
  gameRunning: boolean
  executableExists: boolean
  installDirectory: string
  trayPath: string
  trayInstalled: boolean
  trayRunning: boolean
  installed: boolean
  installedVersion: string | null
  configured: boolean
  armed: boolean
  runtimeManifest: string | null
  changes: string[]
  warnings: string[]
}

export interface OfxrResult {
  installed: boolean
  started: boolean
  armed: boolean
  version: string
  backend: string
  transaction: TransactionRecord | null
}
