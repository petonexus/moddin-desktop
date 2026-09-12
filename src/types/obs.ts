export interface ObsVrRequest {
  gameId: string
  gameName: string
  collectionName: string
  sceneName: string
  sourceName: string
  executableName: string
}

export interface ObsVrPreview {
  canApply: boolean
  obsRunning: boolean
  gameRunning: boolean
  collectionFile: string | null
  collectionName: string | null
  sceneFound: boolean
  templateSourceName: string | null
  sourceExists: boolean
  sourceTargetMatches: boolean
  sourceInScene: boolean
  installed: boolean
  changes: string[]
  warnings: string[]
}
