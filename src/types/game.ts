export type ModuleCategory = 'vr' | 'graphics' | 'qol' | 'system'
export type ModuleStatus = 'available' | 'planned'
export type GameStore = 'steam' | 'epic' | 'gog'

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
  gogAppId?: string
  executable: string
  /**
   * Optional id of an engine preset declared under
   * `src/catalog/engines/<id>.yaml`. The catalog loader merges the
   * preset's modules into the game's module list at load time. Per-game
   * module entries always win over preset entries with the same id.
   */
  enginePreset?: string
  /**
   * Optional PCGamingWiki page title (canonical, may contain spaces).
   * When set, the UI can fetch a cached `PcgwSummary` to enrich the
   * game detail view. When absent, Moddin never queries the wiki.
   */
  pcgwSlug?: string
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