import { describe, expect, it } from 'vitest'
import {
  findCatalogGameByEpicAppId,
  findCatalogGameByGogAppId,
  findCatalogGameById,
  findCatalogGameByInstalledGame,
  findCatalogGameBySteamAppId,
  findCatalogModule,
  findInstalledGameForCatalogGame,
  gameCatalog,
  getMissingDependencies,
  getMissingDependenciesForModule,
  resolveCatalogConfig,
} from '../catalog'
import type { GameCatalogEntry, InstalledGame, ToolModuleDefinition } from '../../types/game'

function module(id: string, dependencies?: string[]): ToolModuleDefinition {
  return {
    id,
    name: id,
    description: `${id} module`,
    category: 'qol',
    status: 'available',
    ...(dependencies ? { dependencies } : {}),
  }
}

describe('gameCatalog', () => {
  it('loads every game YAML and sorts the result by display name', () => {
    const names = gameCatalog.map((game) => game.name)
    expect(names).toEqual([...names].sort((a, b) => a.localeCompare(b)))
    expect(gameCatalog.length).toBeGreaterThanOrEqual(6)
  })

  it('merges the engine preset into each game that declares one', () => {
    // `doom-2016` declares only itself; the idtech preset adds
    // `desktop-shortcut`. If the merge ever stops running, this is the
    // assertion that goes red.
    const doom = findCatalogGameById('doom-2016')
    expect(doom?.enginePreset).toBe('idtech')
    expect(doom?.modules.map((item) => item.id)).toContain('desktop-shortcut')
  })

  it('keeps the game module when a preset declares the same id', () => {
    // `unity` ships `optiscaler` too; the game's own declaration wins
    // verbatim (mergePresetIntoGame's first rule).
    const game = findCatalogGameById('stalker-2')
    const optiscaler = game?.modules.find((item) => item.id === 'optiscaler')
    expect(optiscaler).toBeDefined()
    const duplicates = game?.modules.filter((item) => item.id === 'optiscaler') ?? []
    expect(duplicates).toHaveLength(1)
  })

  it('gives every game a unique id and at least one store app id', () => {
    const ids = gameCatalog.map((game) => game.id)
    expect(new Set(ids).size).toBe(ids.length)
    for (const game of gameCatalog) {
      expect(Boolean(game.steamAppId || game.epicAppId || game.gogAppId)).toBe(true)
    }
  })
})

describe('store lookup', () => {
  it('resolves a catalog game from each store app id', () => {
    expect(findCatalogGameBySteamAppId('1245620')?.id).toBe('elden-ring')
    expect(findCatalogGameByEpicAppId('Crow')?.id).toBe('dead-island-2')
  })

  it('returns undefined for an app id no game declares', () => {
    expect(findCatalogGameBySteamAppId('0')).toBeUndefined()
    expect(findCatalogGameByGogAppId('0')).toBeUndefined()
    expect(findCatalogGameById('not-a-game')).toBeUndefined()
  })

  it('routes an installed game through the right store index', () => {
    const steam: Pick<InstalledGame, 'store' | 'appId'> = { store: 'steam', appId: '1245620' }
    const epic: Pick<InstalledGame, 'store' | 'appId'> = { store: 'epic', appId: 'Crow' }
    expect(findCatalogGameByInstalledGame(steam)?.id).toBe('elden-ring')
    expect(findCatalogGameByInstalledGame(epic)?.id).toBe('dead-island-2')
  })

  it('does not match a Steam app id against the Epic index', () => {
    // The store is the index selector. Looking `1245620` up in the Epic
    // map must not "helpfully" fall back to a Steam match.
    const game: Pick<InstalledGame, 'store' | 'appId'> = { store: 'epic', appId: '1245620' }
    expect(findCatalogGameByInstalledGame(game)).toBeUndefined()
  })
})

describe('findInstalledGameForCatalogGame', () => {
  // P0-5: both AI install entry points used to match a *catalog* id
  // against `InstalledGame.appId` (a store id), so every install ended at
  // "no game selected". This is the regression guard for that.
  const installed: Pick<InstalledGame, 'store' | 'appId' | 'installDir'>[] = [
    { store: 'steam', appId: '1245620', installDir: 'C:\\games\\elden-ring' },
    { store: 'steam', appId: '1091500', installDir: 'C:\\games\\cyberpunk' },
  ]

  it('bridges a catalog id to the store-scanned install directory', () => {
    const found = findInstalledGameForCatalogGame(installed, 'elden-ring')
    expect(found?.installDir).toBe('C:\\games\\elden-ring')
  })

  it('picks the matching game rather than the first one in the list', () => {
    const found = findInstalledGameForCatalogGame(installed, 'cyberpunk-2077')
    expect(found?.appId).toBe('1091500')
  })

  it('returns undefined for a catalog id no game declares', () => {
    expect(findInstalledGameForCatalogGame(installed, 'not-a-game')).toBeUndefined()
  })

  it('returns undefined when the game is not installed', () => {
    expect(findInstalledGameForCatalogGame(installed, 'doom-2016')).toBeUndefined()
  })

  it('round-trips with findCatalogGameByInstalledGame', () => {
    for (const game of installed) {
      const catalogId = findCatalogGameByInstalledGame(game)?.id
      expect(catalogId).toBeDefined()
      expect(findInstalledGameForCatalogGame(installed, catalogId!)?.appId).toBe(game.appId)
    }
  })
})

describe('resolveCatalogConfig', () => {
  // The whole reason this function exists: the per-game `config:` block is
  // the answer to "what values does this recipe need for this game", and
  // two install paths were sending `{}` while the answer sat in the
  // catalogue. These read the real shipped YAML, so a renamed key or a
  // dropped `config:` block goes red here rather than at an install.
  it('returns the per-game config values a recipe needs', () => {
    const config = resolveCatalogConfig('cyberpunk-2077', 'optiscaler')
    expect(config.version).toBe('0.9.4')
    expect(config.downloadUrl).toContain('Optiscaler_0.9.4')
    expect(config.sha256).toMatch(/^[0-9a-f]{64}$/)
    expect(config.proxyCandidates).toBe('dxgi.dll,wininet.dll')
  })

  it('is per game, not global: the same recipe answers per game', () => {
    // `obs-vr` is the clean case: the same module, a different answer for
    // each game. A resolver that read the first match it found would
    // point Cyberpunk's capture at Dawnwalker's executable.
    const cyberpunk = resolveCatalogConfig('cyberpunk-2077', 'obs-vr')
    const dawnwalker = resolveCatalogConfig('dawnwalker', 'obs-vr')
    expect(cyberpunk.sourceName).toBe('Cyberpunk 2077 VR')
    expect(dawnwalker.sourceName).toBe('The Blood of Dawnwalker VR')
    expect(cyberpunk.executableName).toBe('Cyberpunk2077.exe')
  })

  it('returns a copy, so a caller cannot write back into the catalogue', () => {
    const config = resolveCatalogConfig('cyberpunk-2077', 'optiscaler')
    config.version = 'tampered'
    expect(resolveCatalogConfig('cyberpunk-2077', 'optiscaler').version).toBe('0.9.4')
  })

  it('returns nothing rather than a guess for a game or module it does not carry', () => {
    // The honest "I do not know" cases. A caller that finds an empty
    // object asks the user; a caller that found a value would be sending
    // an invented one.
    //
    // `uevr` is the right unknown here rather than `ofxr-bridge`: the
    // latter used to be the example, and it stopped being true when the
    // catalogue gained real ofxr-bridge config. A test that pins "absent"
    // to an id whose absence was a bug will fail the day the bug is
    // fixed, and the fix is what it was waiting for.
    expect(resolveCatalogConfig('cyberpunk-2077', 'uevr')).toEqual({})
    expect(resolveCatalogConfig('not-a-game', 'optiscaler')).toEqual({})
    expect(resolveCatalogConfig(null, 'optiscaler')).toEqual({})
  })

  it('finds the module the game page renders, preset merge included', () => {
    const module = findCatalogModule('doom-2016', 'desktop-shortcut')
    expect(module?.id).toBe('desktop-shortcut')
    expect(findCatalogModule('doom-2016', 'not-declared')).toBeUndefined()
  })
})

describe('getMissingDependencies', () => {
  const game: Pick<GameCatalogEntry, 'modules'> = {
    modules: [module('ui'), module('runtime', ['loader']), module('loader'), module('standalone')],
  }

  it('returns nothing when every module is installed', () => {
    expect(getMissingDependencies(game, ['ui', 'runtime', 'loader', 'standalone'])).toEqual([])
  })

  it('lists a dependency before the module that requires it', () => {
    const missing = getMissingDependencies(game, [])
    expect(missing.indexOf('loader')).toBeLessThan(missing.indexOf('runtime'))
    expect(missing).toContain('ui')
    expect(missing).toContain('standalone')
  })

  it('omits a module that is already installed', () => {
    // `runtime` is installed, so it is not offered again. `loader` is a
    // declared root in its own right, so it is still listed when it is
    // genuinely absent — the result is "everything the game declares
    // that is not installed yet", in dependency order.
    const missing = getMissingDependencies(game, ['runtime'])
    expect(missing).not.toContain('runtime')
    expect(missing).toContain('loader')
    expect(missing).toContain('ui')
  })

  it('does not re-offer an already-installed dependency of a missing module', () => {
    const missing = getMissingDependencies(game, ['loader'])
    expect(missing).not.toContain('loader')
    // `runtime` is itself missing, so it stays in the list; only its
    // installed dependency is pruned from the traversal.
    expect(missing).toContain('runtime')
  })

  it('ignores a dependency id the game does not declare', () => {
    const orphan: Pick<GameCatalogEntry, 'modules'> = {
      modules: [module('plugin', ['not-declared'])],
    }
    expect(getMissingDependencies(orphan, [])).toEqual(['plugin'])
  })

  it('stays finite when the declared dependencies form a cycle', () => {
    const cyclic: Pick<GameCatalogEntry, 'modules'> = {
      modules: [module('a', ['b']), module('b', ['a'])],
    }
    const missing = getMissingDependencies(cyclic, [])
    expect(missing).toHaveLength(2)
    expect([...missing].sort()).toEqual(['a', 'b'])
  })

  it('does not repeat a module reached through two paths', () => {
    const shared: Pick<GameCatalogEntry, 'modules'> = {
      modules: [module('core'), module('left', ['core']), module('right', ['core'])],
    }
    const missing = getMissingDependencies(shared, [])
    expect(missing.filter((id) => id === 'core')).toHaveLength(1)
  })
})

describe('getMissingDependenciesForModule', () => {
  const game: Pick<GameCatalogEntry, 'modules'> = {
    modules: [module('app', ['engine']), module('engine', ['loader']), module('loader'), module('unrelated')],
  }

  it('ignores modules the game declares but the target does not need', () => {
    expect(getMissingDependenciesForModule(game, 'app', [])).not.toContain('unrelated')
  })

  it('returns an empty list for a module the game does not declare', () => {
    expect(getMissingDependenciesForModule(game, 'not-declared', [])).toEqual([])
  })

  it('returns nothing once the whole chain is installed', () => {
    expect(getMissingDependenciesForModule(game, 'app', ['app', 'engine', 'loader'])).toEqual([])
  })

  it('excludes the module itself from its own prerequisite list', () => {
    expect(getMissingDependenciesForModule(game, 'app', [])).toEqual(['loader', 'engine'])
  })

  it('returns only the transitive prerequisites when none are installed', () => {
    expect(getMissingDependenciesForModule(game, 'app', [])).not.toContain('app')
  })

  it('returns only the uninstalled prerequisites', () => {
    expect(getMissingDependenciesForModule(game, 'app', ['loader'])).toEqual(['engine'])
  })
})
