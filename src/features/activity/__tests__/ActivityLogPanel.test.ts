import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import ActivityLogPanel from '../ActivityLogPanel.vue'
import { activityCopyForLocale } from '../copy'
import { i18n } from '../../../i18n'
import ptBR from '../../../i18n/locales/pt-BR'
import type { ActionLogEntry } from '../types'

/**
 * ROADMAP UX-21 for `entry.action`, and UX-27/29 for the panel's own
 * controls.
 *
 * `../service` is mocked, so the rows arrive in the shape the real
 * service hands over: the command name as `text`, the locale key beside
 * it when the table declares one. The panel's job is to render the key
 * and keep the text only as the fallback and as what search matches.
 *
 * The dialog is a Teleport to `document.body`, so the log rows are found
 * through the document, the way a user finds them.
 */

vi.mock('../service', () => ({
  listActionLogs: vi.fn(),
  clearActionLogs: vi.fn(),
}))

import { clearActionLogs, listActionLogs } from '../service'

const mockedList = vi.mocked(listActionLogs)
const mockedClear = vi.mocked(clearActionLogs)
const copy = activityCopyForLocale('en')

enableAutoUnmount(afterEach)

function entry(overrides: Partial<ActionLogEntry> = {}): ActionLogEntry {
  return {
    id: 'log-1',
    timestamp: 1_700_000_000_000,
    level: 'success',
    action: { text: 'install_optiscaler', key: 'activityActionInstallOptiscaler' },
    gameId: 'stalker-2',
    transactionId: 'tx-1',
    message: 'OptiScaler 0.9.4 installed.',
    details: {},
    ...overrides,
  }
}

function panel() {
  return document.body.querySelector('.activity-list') as HTMLElement | null
}

function rows() {
  return Array.from(document.body.querySelectorAll('.activity-entry'))
}

beforeEach(() => {
  i18n.global.locale.value = 'en'
  mockedList.mockReset()
  mockedList.mockResolvedValue([entry()])
  mockedClear.mockReset()
})

async function openPanel(entries: ActionLogEntry[] = [entry()]) {
  mockedList.mockResolvedValue(entries)
  const wrapper = mount(ActivityLogPanel, {
    attachTo: document.body,
    global: { plugins: [i18n] },
  })
  await wrapper.find('.nav-item').trigger('click')
  await flushPromises()
  return wrapper
}

describe('the action column', () => {
  it('renders the locale sentence, not the command name', async () => {
    await openPanel()

    expect(rows()[0].textContent).toContain('Install OptiScaler')
    expect(rows()[0].textContent).not.toContain('install_optiscaler')
  })

  it('renders the pt-BR sentence when the app is in pt-BR', async () => {
    const wrapper = mount(ActivityLogPanel, {
      attachTo: document.body,
      global: { plugins: [i18n] },
    })
    i18n.global.locale.value = 'pt-BR'
    await wrapper.find('.nav-item').trigger('click')
    await flushPromises()

    expect(rows()[0].textContent).toContain(ptBR.activityActionInstallOptiscaler)
  })

  it('falls back to the command name for a command nobody declared', async () => {
    await openPanel([entry({ action: { text: 'capability_install' } })])

    // The documented fallback: the backend's own text, not a blank row.
    expect(rows()[0].textContent).toContain('capability_install')
  })

  it('still matches the command name in the search box', async () => {
    await openPanel()
    const search = document.body.querySelector('#activity-search') as HTMLInputElement

    // The row shows a translated label now, but someone who remembers
    // `install_optiscaler` still has to be able to find it.
    search.value = 'install_optiscaler'
    search.dispatchEvent(new Event('input'))
    await flushPromises()
    expect(rows()).toHaveLength(1)

    search.value = 'nothing-matches-this'
    search.dispatchEvent(new Event('input'))
    await flushPromises()
    expect(rows()).toHaveLength(0)
  })
})

describe('the toolbar controls are named', () => {
  it('gives the search box and the level filter a visually hidden label', async () => {
    await openPanel()

    const search = document.body.querySelector('label[for="activity-search"]') as HTMLElement
    const level = document.body.querySelector('label[for="activity-level"]') as HTMLElement
    expect(search.className).toContain('sr-only')
    expect(level.className).toContain('sr-only')
    expect(search.textContent).toBe(copy.searchLabel)
    expect(level.textContent).toBe(copy.levelLabel)
  })

  it('marks the list busy while a refresh is in flight', async () => {
    await openPanel()
    expect(panel()?.getAttribute('aria-busy')).toBe('false')

    let release!: (rows: ActionLogEntry[]) => void
    mockedList.mockImplementationOnce(() => new Promise((resolve) => { release = resolve }))
    ;(document.body.querySelector('.activity-toolbar .btn') as HTMLButtonElement).click()
    await flushPromises()
    expect(panel()?.getAttribute('aria-busy')).toBe('true')

    release([entry()])
    await flushPromises()
    expect(panel()?.getAttribute('aria-busy')).toBe('false')
  })
})

describe('activity filtering and clearing', () => {
  it('searches the translated action shown to the player', async () => {
    await openPanel()
    const search = document.body.querySelector('#activity-search') as HTMLInputElement
    search.value = 'Install OptiScaler'
    search.dispatchEvent(new Event('input'))
    await flushPromises()
    expect(rows()).toHaveLength(1)
  })

  it('offers filter recovery without claiming the log is empty', async () => {
    await openPanel()
    const level = document.body.querySelector('#activity-level') as HTMLSelectElement
    level.value = 'error'
    level.dispatchEvent(new Event('change'))
    await flushPromises()

    const empty = document.body.querySelector('.empty-state') as HTMLElement
    expect(empty.textContent).toContain(copy.filteredEmptyTitle)
    expect(empty.textContent).not.toContain(copy.empty)
    ;(empty.querySelector('button') as HTMLButtonElement).click()
    await flushPromises()
    expect(rows()).toHaveLength(1)
    expect(level.value).toBe('all')
  })

  it('confirms clearing in the app, protects the operation and keeps the parent open', async () => {
    const wrapper = await openPanel()
    expect(wrapper.find('.nav-item').attributes('aria-expanded')).toBe('true')
    ;(document.body.querySelector('.dialog-footer .btn-danger') as HTMLButtonElement).click()
    await flushPromises()

    expect(mockedClear).not.toHaveBeenCalled()
    const dialogs = document.body.querySelectorAll('[role="dialog"]')
    expect(dialogs).toHaveLength(2)
    expect(dialogs[1].textContent).toContain(copy.confirmClear)
    ;(dialogs[1].querySelector('.dialog-footer .btn') as HTMLButtonElement).click()
    await flushPromises()
    expect(mockedClear).not.toHaveBeenCalled()
    expect(document.body.querySelectorAll('[role="dialog"]')).toHaveLength(1)

    ;(document.body.querySelector('.dialog-footer .btn-danger') as HTMLButtonElement).click()
    await flushPromises()
    let release!: () => void
    mockedClear.mockImplementationOnce(() => new Promise((resolve) => { release = resolve }))
    ;(document.body.querySelectorAll('[role="dialog"]')[1].querySelector('.btn-danger-solid') as HTMLButtonElement).click()
    await flushPromises()
    expect(mockedClear).toHaveBeenCalledTimes(1)
    const busyConfirmation = document.body.querySelectorAll('[role="dialog"]')[1]
    expect(busyConfirmation.getAttribute('aria-busy')).toBe('true')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(document.body.querySelectorAll('[role="dialog"]')).toHaveLength(2)

    release()
    await flushPromises()
    expect(document.body.querySelectorAll('[role="dialog"]')).toHaveLength(1)
    expect(rows()).toHaveLength(0)
    expect(document.body.querySelector('.empty-state')?.textContent).toContain(copy.empty)
    expect(wrapper.find('.nav-item').attributes('aria-expanded')).toBe('true')
  })
})
