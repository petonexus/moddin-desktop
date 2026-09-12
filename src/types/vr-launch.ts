import type { TransactionRecord } from './transaction'

export interface VrRecommendation {
  label: string
  value: string
}

export interface VrIniPatch {
  section: string
  key: string
  value: string
}

export interface VrLaunchRequest {
  gameId: string
  gameName: string
  installDir: string
  executable: string
  arguments: string[]
  requiredFiles: string[]
  configPath: string | null
  configPatches: VrIniPatch[]
  recommendations: VrRecommendation[]
  safetyNotes: string[]
}

export interface VrSettingStatus extends VrIniPatch {
  currentValue: string | null
  willChange: boolean
}

export interface VrLaunchPreview {
  canLaunch: boolean
  gameRunning: boolean
  executablePath: string
  executableDirectory: string
  activeOpenXrRuntime: string | null
  missingFiles: string[]
  configPath: string | null
  configExists: boolean
  settings: VrSettingStatus[]
  changes: string[]
  warnings: string[]
}

export interface VrLaunchResult {
  processId: number
  transaction: TransactionRecord | null
}
