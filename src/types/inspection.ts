export interface ProxyDllInfo {
  name: string
  path: string
  sizeBytes: number
}

export interface GameEnvironmentInspection {
  executablePath: string
  executableDirectory: string
  executableExists: boolean
  gameRunning: boolean
  proxyDlls: ProxyDllInfo[]
  engine: string | null
  engineVersion: string | null
  engineConfidence: string
  engineEvidence: string[]
}
