export type ModuleCategory = 'vr' | 'graphics' | 'qol' | 'system'
export type ModuleStatus = 'available' | 'planned'

export interface ToolModuleDefinition {
  id: string
  name: string
  description: string
  category: ModuleCategory
  status: ModuleStatus
  config?: Record<string, string>
}

export interface GameCatalogEntry {
  id: string
  name: string
  steamAppId: string
  executable: string
  modules: ToolModuleDefinition[]
}

export interface InstalledGame {
  appId: string
  name: string
  installDir: string
  libraryPath: string
}
