import type { InstalledGame } from '../../types/game'

function searchText(value: string) {
  return value.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLocaleLowerCase().trim()
}

export function filterInstalledGames(
  games: InstalledGame[],
  search: string,
  supportedOnly: boolean,
  isSupported: (game: InstalledGame) => boolean,
  displayName: (game: InstalledGame) => string,
): InstalledGame[] {
  const term = searchText(search)
  return games
    .filter((game) => (!supportedOnly || isSupported(game))
      && (!term || searchText(`${displayName(game)} ${game.name}`).includes(term)))
    .sort((a, b) => Number(isSupported(b)) - Number(isSupported(a))
      || displayName(a).localeCompare(displayName(b)))
}

/** Keep the player's selection after rescans, and recover if it was uninstalled. */
export function selectedDetectedGame(
  games: InstalledGame[],
  previousAppId: string | null,
  isSupported: (game: InstalledGame) => boolean,
): string | null {
  return games.find((game) => game.appId === previousAppId)?.appId
    ?? games.find(isSupported)?.appId
    ?? games[0]?.appId
    ?? null
}
