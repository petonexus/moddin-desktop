export type ModuleCategory = 'vr' | 'graphics' | 'qol' | 'system'
export type ModuleStatus = 'available' | 'planned'
export type GameStore = 'steam' | 'epic'

export interface ToolModuleDefinition {
  id: string
  name: string
  description: string
  category: ModuleCategory
  status: ModuleStatus
  config?: Record<string, string | string[]>
}

export interface GameCatalogEntry {
  id: string
  name: string
  steamAppId?: string
  epicAppId?: string
  executable: string
  /**
   * Optional id of an engine preset declared under
   * `src/catalog/engines/<id>.yaml`. The catalog loader merges the
   * preset's modules into the game's module list at load time. Per-game
   * module entries always win over preset entries with the same id.
   */
  enginePreset?: string
  modules: ToolModuleDefinition[]
}

export interface EnginePreset {
  id: string
  displayName: string
  description: string
  modules: ToolModuleDefinition[]
}

export interface InstalledGame {
  store: GameStore
  appId: string
  name: string
  installDir: string
  libraryPath: string
}