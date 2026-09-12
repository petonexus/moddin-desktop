import type { TransactionRecord } from './transaction'

export type UevrBackend = 'nightly' | 'joey' | 'afw' | 'joey-afw'
export type UevrBackendCompatibility = 'preferred' | 'available' | 'experimental' | 'unknown' | 'not_working'

export interface UevrRequest {
  gameId: string
  gameName: string
  installDir: string
  executable: string
  backend: UevrBackend
  versionPolicy: 'latest' | 'pinned'
  releaseApiUrl: string
  releaseTag: string | null
  backendReleaseApiUrl: string | null
  backendReleaseTag: string | null
  safetyNotes: string[]
}

export interface UevrPreview {
  canApply: boolean
  gameRunning: boolean
  uevrRunning: boolean
  executableExists: boolean
  executablePath: string
  executableDirectory: string
  engine: string | null
  engineVersion: string | null
  engineConfidence: string
  engineEvidence: string[]
  backend: UevrBackend
  backendLabel: string
  selectedVersion: string | null
  installed: boolean
  installedVersion: string | null
  installedBackend: UevrBackend | null
  installDirectory: string
  manualInstallDetected: boolean
  changes: string[]
  warnings: string[]
}

export type UevrResult = TransactionRecord
