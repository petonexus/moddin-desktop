import { describe, expect, it } from 'vitest'
import { filterInstalledGames, selectedDetectedGame } from '../library-view'
import type { InstalledGame } from '../../../types/game'

const games: InstalledGame[] = [
  { appId: '1', name: 'Nome da loja', store: 'steam', installDir: 'C:/Games/1', libraryPath: 'C:/Games' },
  { appId: '2', name: 'Árvore', store: 'gog', installDir: 'C:/Games/2', libraryPath: 'C:/Games' },
  { appId: '3', name: 'Outro jogo', store: 'epic', installDir: 'C:/Games/3', libraryPath: 'C:/Games' },
]
const supported = (game: InstalledGame) => game.appId !== '3'
const displayName = (game: InstalledGame) => game.appId === '1' ? 'Título do catálogo' : game.name

describe('library discovery and selection', () => {
  it('finds displayed catalog titles and original store names without requiring accents', () => {
    expect(filterInstalledGames(games, ' TITULO ', true, supported, displayName).map((game) => game.appId)).toEqual(['1'])
    expect(filterInstalledGames(games, 'nome da loja', false, supported, displayName).map((game) => game.appId)).toEqual(['1'])
    expect(filterInstalledGames(games, 'arvore', false, supported, displayName).map((game) => game.appId)).toEqual(['2'])
  })

  it('keeps supported games first, orders by the displayed title, and does not reorder the source', () => {
    expect(filterInstalledGames(games, '', false, supported, displayName).map((game) => game.appId)).toEqual(['2', '1', '3'])
    expect(games.map((game) => game.appId)).toEqual(['1', '2', '3'])
    expect(filterInstalledGames(games, '', true, supported, displayName).map((game) => game.appId)).toEqual(['2', '1'])
  })

  it('preserves a remembered game and recovers when it is no longer installed', () => {
    expect(selectedDetectedGame(games, '3', supported)).toBe('3')
    expect(selectedDetectedGame(games, 'uninstalled', supported)).toBe('1')
    expect(selectedDetectedGame([games[2]!], null, supported)).toBe('3')
    expect(selectedDetectedGame([], '1', supported)).toBeNull()
  })
})
