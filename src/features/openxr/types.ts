import type { LocalizedText } from '../../i18n/backendIds'

export interface OpenXrRuntimeInfo {
  name: string
  manifestPath: string
  libraryPath: string | null
  manifestExists: boolean
  libraryExists: boolean
  enabled: boolean
  active: boolean
}

/** The state as `openxr.rs` writes it, before the UX-21 boundary. */
export type OpenXrStateWire = Omit<OpenXrState, 'warnings'> & {
  warnings: string[]
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
  /**
   * UX-21: these are the backend's own diagnosis sentences — a missing
   * manifest, a registry value pointing at nothing. They name the thing
   * the user has to check next, so they are translated rather than
   * dropped; `text` is kept for the documented fallback.
   */
  warnings: LocalizedText[]
}
