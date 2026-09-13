export interface OpenXrRuntimeInfo {
  name: string
  manifestPath: string
  libraryPath: string | null
  manifestExists: boolean
  libraryExists: boolean
  enabled: boolean
  active: boolean
}

export interface OpenXrState {
  activeRuntime: string | null
  activeRuntimeName: string | null
  gameOverride: string | null
  gameOverrideName: string | null
  effectiveRuntime: string | null
  effectiveRuntimeName: string | null
  effectiveSource: 'game' | 'system' | 'none'
  runtimes: OpenXrRuntimeInfo[]
  warnings: string[]
}
