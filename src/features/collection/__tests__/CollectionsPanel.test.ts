import { shallowRef } from 'vue'
import { selectedGameKey, type SelectedGameContext } from '../../../composables/useSelectedGame'
const selection = shallowRef<SelectedGameContext | null>(null)
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import CollectionsPanel from '../../../components/shell/CollectionsPanel.vue'
import { i18n } from '../../../i18n'
import type { CollectionSummary } from '../service'

/**
 * The panel against the wire shape.
 *
 * The fixtures below are `collection_list`'s output field for field, so
 * the panel is exercised against the real contract rather than against a
 * convenient one: the shipped sets have to render, the empty state has to
 * mean "the loader had none", and a set the loader refused has to be
 * reported and refused again by the panel instead of walking the user
 * into an install that fails on its first member.
 *
 * The service is mocked rather than the bridge behind it — that is the
 * boundary `check:architecture` draws, and the same one the panel's other
 * test file draws. What the loader actually sends is pinned on the Rust
 * side, by tests that parse the shipped collection files and build the
 * summaries from the shipped catalogue.
 */
vi.mock('../../../features/collection/service', () => ({ listCollections: vi.fn() }))
vi.mock('../../../services/catalog', () => ({ findCatalogGameById: vi.fn(() => null) }))

import { listCollections } from '../service'
import { findCatalogGameById } from '../../../services/catalog'

const mockedList = vi.mocked(listCollections)
const mockedFindGame = vi.mocked(findCatalogGameById)

enableAutoUnmount(afterEach)

// The panel and its dialogs attach to `document.body`; a dialog left
// behind by one test would be found by the next one's
// `document.body.querySelector('[role="dialog"]')`.
afterEach(() => {
  document.body.innerHTML = ''
})

beforeEach(() => {
  i18n.global.locale.value = 'en'
  mockedList.mockReset()
  mockedFindGame.mockReset()
  selection.value = { appId: '1245620', gameId: 'elden-ring', gameName: 'Elden Ring', engine: null }
  mockedFindGame.mockReturnValue({
    id: 'elden-ring',
    name: 'Elden Ring',
  } as unknown as ReturnType<typeof findCatalogGameById>)
})

/** A summary exactly as `collection_list` serialises one. */
function summary(overrides: Partial<CollectionSummary> = {}): CollectionSummary {
  return {
    id: 'vr-cyberpunk-2077-stack',
    displayName: 'Cyberpunk 2077 VR performance stack',
    category: 'vr',
    description: 'Upscaling for the flat view, frame generation in the headset.',
    targetGame: 'cyberpunk-2077',
    capabilityCount: 2,
    requiredCount: 2,
    preset: {
      blocked: [],
      capabilities: [
        { id: 'optiscaler', displayName: 'OptiScaler DLSS upscaler', rationale: 'Upscaling.' },
        { id: 'ofxr-bridge', displayName: 'OFXR Bridge FrameGen', rationale: 'Frame generation.' },
      ],
    },
    ...overrides,
  }
}

function optiscalerOnly() {
  return [{ id: 'optiscaler', displayName: 'OptiScaler DLSS upscaler', rationale: 'Upscaling.' }]
}

async function openPanel() {
  const wrapper = mount(CollectionsPanel, { attachTo: document.body, global: { plugins: [i18n], provide: { [selectedGameKey as symbol]: selection } } })
  await wrapper.find('button.nav-item').trigger('click')
  await flushPromises()
  return wrapper
}

function rowInstallButtons() {
  return Array.from(
    document.body.querySelectorAll<HTMLButtonElement>('.collections-entry-actions button'),
  )
}

describe('CollectionsPanel — the list the loader sent', () => {
  it('reloads verdicts with the catalog id when the library selection changes', async () => {
    mockedList.mockResolvedValue([])
    await openPanel()
    expect(mockedList).toHaveBeenLastCalledWith('elden-ring')
    selection.value = { appId: '1091500', gameId: 'cyberpunk-2077', gameName: 'Cyberpunk 2077', engine: null }
    await flushPromises()
    expect(mockedList).toHaveBeenLastCalledWith('cyberpunk-2077')
  })

  it('does not show a stale collection response after a game change', async () => {
    let resolveOlder!: (value: CollectionSummary[]) => void
    mockedList.mockImplementationOnce(() => new Promise((resolve) => { resolveOlder = resolve }))
    await openPanel()
    mockedList.mockResolvedValue([])
    selection.value = { appId: '1091500', gameId: 'cyberpunk-2077', gameName: 'Cyberpunk 2077', engine: null }
    await flushPromises()
    resolveOlder([summary()])
    await flushPromises()
    expect(document.body.querySelectorAll('.collections-entry')).toHaveLength(0)
    expect(document.body.querySelector('.empty-state')).not.toBeNull()
  })

  it('renders the collections it was given', async () => {
    mockedList.mockResolvedValue([
      summary(),
      summary({
        id: 'vr-stalker-2-stack',
        displayName: 'S.T.A.L.K.E.R. 2 VR performance stack',
        targetGame: 'stalker-2',
        preset: { blocked: [], capabilities: optiscalerOnly() },
      }),
    ])
    await openPanel()

    const text = document.body.textContent ?? ''
    expect(text).toContain('Cyberpunk 2077 VR performance stack')
    expect(text).toContain('S.T.A.L.K.E.R. 2 VR performance stack')
    // The counts came from the wire, not from a guess.
    expect(text).toContain('2 capabilities')
    expect(document.body.querySelectorAll('.collections-entry')).toHaveLength(2)
    // Two real collections, so the empty state must not be on screen.
    expect(document.body.querySelector('.empty-state')).toBeNull()
  })

  it('shows the empty state only when the loader had none', async () => {
    mockedList.mockResolvedValue([])
    await openPanel()

    const empty = document.body.querySelector('.empty-state')
    expect(empty).not.toBeNull()
    expect(empty?.getAttribute('role')).toBe('status')
    expect(empty?.textContent).toContain('No collections available yet')
    expect(document.body.querySelector('.collections-list')).toBeNull()
    expect(document.body.querySelector('.collections-blocked')).toBeNull()
  })

  it('opens the wizard for a set the loader did not refuse', async () => {
    // The control for the guard below: a clean set still installs. The
    // list and the wizard are the same dialog — the wizard replaces the
    // list rather than stacking on it — so "the wizard opened" is the
    // plan being on screen, not a second dialog.
    mockedList.mockResolvedValue([summary({ targetGame: 'elden-ring' })])
    await openPanel()

    expect(document.body.querySelector('.collections-blocked')).toBeNull()
    rowInstallButtons()[0].click()
    await flushPromises()

    expect(document.body.querySelector('.collection-plan-list')).not.toBeNull()
    expect(document.body.textContent).toContain('OFXR Bridge FrameGen')
  })
})

describe('CollectionsPanel — a set the loader refused', () => {
  it('names a member the engine gate hides and does not open the wizard', async () => {
    mockedList.mockResolvedValue([
      summary({
        targetGame: 'elden-ring',
        preset: {
          blocked: [
            {
              kind: 'engineMismatch',
              capabilityId: 'bepinex',
              supportedEngines: ['unity'],
              targetGame: null,
            },
          ],
          capabilities: [
            { id: 'bepinex', displayName: 'BepInEx', rationale: 'Mod loader.' },
            { id: 'uevr', displayName: 'UEVR', rationale: 'VR injector.' },
          ],
        },
      }),
    ])
    await openPanel()

    // Reported, by name, with the reason the loader gave.
    const note = document.body.querySelector('.collections-blocked')
    expect(note).not.toBeNull()
    expect(note?.textContent).toContain('Not installable into Elden Ring')
    expect(note?.textContent).toContain('BepInEx')
    expect(note?.textContent).toContain('unity')
    // The set is still listed: the panel is a catalogue, and the refusal
    // is about this game, not about whether the set exists.
    expect(document.body.querySelectorAll('.collections-entry')).toHaveLength(1)

    rowInstallButtons()[0].click()
    await flushPromises()

    // The list is still up and the wizard never opened. It would have
    // walked the members in order and failed on BepInEx with files
    // already written.
    expect(document.body.querySelector('.collections-list')).not.toBeNull()
    expect(document.body.querySelector('.collection-plan-list')).toBeNull()
  })

  it('names the game a set was curated for', async () => {
    mockedList.mockResolvedValue([
      summary({
        preset: {
          blocked: [
            {
              kind: 'wrongGame',
              capabilityId: null,
              supportedEngines: [],
              targetGame: 'cyberpunk-2077',
            },
          ],
          capabilities: optiscalerOnly(),
        },
      }),
    ])
    await openPanel()

    const note = document.body.querySelector('.collections-blocked')?.textContent ?? ''
    expect(note).toContain('Curated for cyberpunk-2077')
    expect(note).toContain('Elden Ring')

    rowInstallButtons()[0].click()
    await flushPromises()
    expect(document.body.querySelector('.collection-plan-list')).toBeNull()
  })

  it('reports nothing while no game is selected', async () => {
    selection.value = null
    mockedFindGame.mockReturnValue(undefined)
    mockedList.mockResolvedValue([
      summary({
        preset: {
          blocked: [
            {
              kind: 'wrongGame',
              capabilityId: null,
              supportedEngines: [],
              targetGame: 'cyberpunk-2077',
            },
          ],
          capabilities: optiscalerOnly(),
        },
      }),
    ])
    await openPanel()

    // No game means no verdict to report, and the wizard's own
    // collectionNoGame copy is what the user is offered instead.
    expect(document.body.querySelector('.collections-blocked')).toBeNull()
  })
})
