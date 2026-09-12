import type { TransactionRecord } from './transaction'

export interface CheekyFoveatedDlssRequest {
  gameId: string
  gameName: string
  installDir: string
  executable: string
  version: string
  downloadUrl: string
  sha256: string
  addonFile: string
  safetyNotes: string[]
}

export interface CheekyFoveatedDlssPreview {
  canApply: boolean
  gameRunning: boolean
  executableExists: boolean
  executablePath: string
  executableDirectory: string
  addonPath: string
  installed: boolean
  installedVersion: string | null
  manualInstallDetected: boolean
  openXrSetupRequired: boolean
  changes: string[]
  warnings: string[]
}

export type CheekyFoveatedDlssResult = TransactionRecord
