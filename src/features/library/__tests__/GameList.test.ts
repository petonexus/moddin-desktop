import { enableAutoUnmount, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { i18n } from '../../../i18n'
import type { InstalledGame } from '../../../types/game'
import GameList from '../GameList.vue'
import { libraryCopyForLocale } from '../copy'

enableAutoUnmount(afterEach)

const games: InstalledGame[] = [
  { appId: 'steam-1', name: 'First game', store: 'steam', installDir: 'C:/Games/First', libraryPath: 'C:/Games' },
  { appId: 'epic-2', name: 'Second game', store: 'epic', installDir: 'C:/Games/Second', libraryPath: 'C:/Games' },
  { appId: 'gog-3', name: 'Third game', store: 'gog', installDir: 'C:/Games/Third', libraryPath: 'C:/Games' },
]

function mountList(overrides: Partial<InstanceType<typeof GameList>['$props']> = {}) {
  return mount(GameList, {
    attachTo: document.body,
    props: {
      games,
      selectedAppId: 'steam-1',
      loading: false,
      totalCount: games.length,
      supportedCount: 2,
      modCount: (game: InstalledGame) => game.appId === 'epic-2' ? 0 : 2,
      displayName: (game: InstalledGame) => game.name,
      search: '',
      supportedOnly: true,
      ...overrides,
    },
    global: { plugins: [i18n] },
  })
}

beforeEach(() => { i18n.global.locale.value = 'en' })

describe('library empty-state recovery', () => {
  it('renders the shared loading state and withholds recovery actions until scanning finishes', () => {
    const wrapper = mountList({ games: [], totalCount: 0, loading: true })
    const empty = wrapper.find('.empty-state')

    expect(empty.exists()).toBe(true)
    expect(empty.attributes('role')).toBe('status')
    expect(empty.attributes('aria-busy')).toBe('true')
    expect(empty.text()).toContain(libraryCopyForLocale('en').scanningLibraries)
    expect(empty.findAll('button')).toHaveLength(0)
    expect(wrapper.find('.rescan-button').attributes('disabled')).toBeDefined()
  })

  it('offers a rescan when no games were detected even if a stale search is present', async () => {
    const wrapper = mountList({ games: [], totalCount: 0, supportedCount: 0, search: 'old search' })
    const empty = wrapper.find('.empty-state')

    expect(empty.text()).toContain(libraryCopyForLocale('en').emptyLibraryTitle)
    expect(empty.text()).toContain('Steam, Epic or GOG')
    await empty.find('button').trigger('click')
    expect(wrapper.emitted('rescan')).toEqual([[]])
  })

  it('explains a missing supported game set and resets both the search and support filter', async () => {
    const wrapper = mountList({ games: [], supportedCount: 0, search: 'old search' })
    const empty = wrapper.find('.empty-state')

    expect(empty.text()).toContain(libraryCopyForLocale('en').emptySupportedTitle)
    const reset = empty.findAll('button').find((button) => button.text() === libraryCopyForLocale('en').resetFilters)!
    await reset.trigger('click')
    expect(wrapper.emitted('update:supportedOnly')).toEqual([[false]])
    expect(wrapper.emitted('update:search')).toEqual([['']])
    expect(document.activeElement).toBe(wrapper.find('input').element)
  })

  it('distinguishes a search with no matches from a library with no detected games', async () => {
    const wrapper = mountList({ games: [], search: 'missing game', supportedOnly: false })
    const empty = wrapper.find('.empty-state')

    expect(empty.text()).toContain(libraryCopyForLocale('en').emptySearchTitle)
    expect(empty.text()).not.toContain(libraryCopyForLocale('en').emptyLibraryTitle)
    await empty.find('button').trigger('click')
    expect(wrapper.emitted('update:search')).toEqual([['']])
    expect(wrapper.emitted('rescan')).toBeUndefined()
  })

  it('offers all detected games when a search is restricted to games with mods', () => {
    const wrapper = mountList({ games: [], search: 'missing game' })
    const empty = wrapper.find('.empty-state')

    expect(empty.text()).toContain(libraryCopyForLocale('en').emptySupportedSearchHint)
    expect(empty.findAll('button').map((button) => button.text())).toContain(libraryCopyForLocale('en').resetFilters)
  })
})

describe('library navigation and context', () => {
  it('shows result counts, store labels and the current selection', () => {
    const wrapper = mountList()
    const rows = wrapper.findAll('.game-row')

    expect(wrapper.find('h2').text()).toBe(libraryCopyForLocale('en').title)
    expect(wrapper.find('.library-results').text()).toBe('3 of 3 games')
    expect(rows.map((row) => row.find('.game-store').text())).toEqual(['Steam', 'Epic Games', 'GOG'])
    expect(rows[0].attributes('aria-current')).toBe('true')
    expect(rows[1].attributes('aria-current')).toBeUndefined()
    expect(rows[1].text()).toContain(libraryCopyForLocale('en').noCompatibleMods)
  })

  it('moves focus with arrow keys, Home and End without silently selecting another game', async () => {
    const wrapper = mountList()
    const rows = wrapper.findAll('.game-row')
    ;(rows[0].element as HTMLButtonElement).focus()

    await rows[0].trigger('keydown', { key: 'ArrowDown' })
    expect(document.activeElement).toBe(rows[1].element)
    await rows[1].trigger('keydown', { key: 'End' })
    expect(document.activeElement).toBe(rows[2].element)
    await rows[2].trigger('keydown', { key: 'Home' })
    expect(document.activeElement).toBe(rows[0].element)
    expect(wrapper.emitted('select')).toBeUndefined()

    await rows[1].trigger('click')
    expect(wrapper.emitted('select')).toEqual([['epic-2']])
  })

  it('clears a search with Escape while preserving the selected support filter', async () => {
    const wrapper = mountList({ search: 'First' })
    const input = wrapper.find('#library-search')
    await input.trigger('keydown', { key: 'Escape' })

    expect(wrapper.emitted('update:search')).toEqual([['']])
    expect(wrapper.emitted('update:supportedOnly')).toBeUndefined()
    expect(document.activeElement).toBe(input.element)
  })

  it('locks game selection and rescanning during a mutation and explains how to resume', async () => {
    const wrapper = mountList({ selectionBusy: true })
    const rows = wrapper.findAll('.game-row')
    const lock = wrapper.find('#library-selection-lock')

    expect(lock.text()).toBe(libraryCopyForLocale('en').selectionBusyHint)
    for (const row of rows) {
      expect(row.attributes('disabled')).toBeDefined()
      expect(row.attributes('aria-describedby')).toBe(lock.attributes('id'))
      await row.trigger('click')
    }
    await wrapper.find('.rescan-button').trigger('click')
    expect(wrapper.emitted('select')).toBeUndefined()
    expect(wrapper.emitted('rescan')).toBeUndefined()

    await wrapper.setProps({ selectionBusy: false })
    expect(wrapper.find('#library-selection-lock').exists()).toBe(false)
    await rows[1].trigger('click')
    expect(wrapper.emitted('select')).toEqual([['epic-2']])
  })

  it.each(['pt-BR', 'es'])('updates the recovery copy with the %s locale', async (locale) => {
    const wrapper = mountList({ games: [], search: 'missing', supportedOnly: false })
    i18n.global.locale.value = locale as 'pt-BR' | 'es'
    await wrapper.vm.$nextTick()

    expect(wrapper.find('.empty-state').text()).toContain(libraryCopyForLocale(locale).emptySearchTitle)
    expect(wrapper.find('.empty-state button').text()).toBe(libraryCopyForLocale(locale).clearSearch)
    expect(wrapper.find('h2').text()).toBe(libraryCopyForLocale(locale).title)
  })
})
