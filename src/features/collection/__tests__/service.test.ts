import { beforeEach, describe, expect, it, vi } from 'vitest'
import { selectedGameRequest } from '../service'

/**
 * The catalogue half of the collections seam.
 *
 * The catalogue is a frontend data set and the engine gate is a backend
 * rule, so something has to join them. This file pins the half that
 * lives here: the selected game leaves as an id and an `enginePreset`,
 * and the loader answers with a verdict. A component that re-derived the
 * rule would be the second implementation of it.
 *
 * These are the boundary's own function rather than the Tauri command
 * around it, and that is deliberate, for the reason
 * `check:architecture` gives: `invokeDebug` belongs to
 * `features/<feature>/service.ts`, so a test that stood the bridge up
 * would be testing that the service calls it rather than what it sends.
 */
vi.mock('../../../services/catalog', () => ({ findCatalogGameById: vi.fn(() => undefined) }))

import { findCatalogGameById } from '../../../services/catalog'

const mockedFindGame = vi.mocked(findCatalogGameById)

beforeEach(() => {
  mockedFindGame.mockReset()
  window.localStorage.clear()
})

describe('selectedGameRequest', () => {
  it('passes the catalogue id and its engine preset', () => {
    window.localStorage.setItem('moddin-selected-appId', 'stalker-2')
    mockedFindGame.mockReturnValue({
      id: 'stalker-2',
      name: 'S.T.A.L.K.E.R. 2',
      enginePreset: 'unreal5',
    } as unknown as ReturnType<typeof findCatalogGameById>)

    expect(selectedGameRequest()).toEqual({ gameId: 'stalker-2', engine: 'unreal5' })
  })

  it('sends no game and no engine when nothing is selected', () => {
    expect(selectedGameRequest()).toEqual({ gameId: null, engine: null })
  })

  it('keeps the id for a game the catalogue does not describe', () => {
    // A library game with no catalogue entry still has an id, and that is
    // what lets the loader say "this set is curated for another game".
    // Only the engine is missing, which is the loader's noGameEngine
    // case: it gates nothing.
    window.localStorage.setItem('moddin-selected-appId', 'steam-481516')
    mockedFindGame.mockReturnValue(undefined)

    expect(selectedGameRequest()).toEqual({ gameId: 'steam-481516', engine: null })
  })

  it('sends a null engine for a game that declares none', () => {
    window.localStorage.setItem('moddin-selected-appId', 'elden-ring')
    mockedFindGame.mockReturnValue({
      id: 'elden-ring',
      name: 'Elden Ring',
    } as unknown as ReturnType<typeof findCatalogGameById>)

    expect(selectedGameRequest()).toEqual({ gameId: 'elden-ring', engine: null })
  })
})
