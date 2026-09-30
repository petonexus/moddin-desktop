import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import CollectionsPanel from '../../shell/CollectionsPanel.vue'
import { i18n } from '../../../i18n'
import type { CollectionSummary } from '../../../types/collection'

/**
 * The collections product line, ported back from `feat/agent-mcp`.
 *
 * The contract under test is the one the port had to honour: a
 * collection is *not* a second install path. It is the same
 * `community_capability_install` command the Community panel and the AI
 * already use, run once per capability, each one recorded by the
 * transaction system. So the tests assert the command, not the panel —
 * plus the confirmation gate in front of it, because that command writes
 * into a game folder.
 */

vi.mock('../../../features/collection/service', () => ({ listCollections: vi.fn() }))
vi.mock('../../../features/community/service', () => ({ installCommunityCapability: vi.fn() }))
vi.mock('../../../features/capability-modules/service', () => ({
  resolveInstallTarget: vi.fn(),
  uninstallCapability: vi.fn(),
}))
vi.mock('../../../services/catalog', () => ({ findCatalogGameById: vi.fn(() => null) }))

import { listCollections } from '../../../features/collection/service'
import { installCommunityCapability } from '../../../features/community/service'
import { resolveInstallTarget, uninstallCapability } from '../../../features/capability-modules/service'
import { findCatalogGameById } from '../../../services/catalog'
import type { CommunityInstallResult } from '../../../features/community/types'

const mockedList = vi.mocked(listCollections)
const mockedInstall = vi.mocked(installCommunityCapability)
const mockedResolve = vi.mocked(resolveInstallTarget)
const mockedUninstall = vi.mocked(uninstallCapability)
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
  mockedInstall.mockReset()
  mockedResolve.mockReset()
  mockedUninstall.mockReset()
  mockedFindGame.mockReset()

  window.localStorage.setItem('moddin-selected-appId', 'elden-ring')
  mockedFindGame.mockReturnValue({
    id: 'elden-ring',
    name: 'Elden Ring',
    executable: 'eldenring.exe',
    modules: [],
  })
  mockedResolve.mockResolvedValue({
    gameId: 'elden-ring',
    gameName: 'Elden Ring',
    installDir: 'C:\\games\\elden-ring\\Mods',
    executableDir: 'C:\\games\\elden-ring',
  })
  mockedInstall.mockResolvedValue({
    capabilityId: 'uevr',
    transaction: { id: 'tx-1' },
    steps: [],
    affectedPaths: [],
  } as CommunityInstallResult)
  mockedList.mockResolvedValue([vrBundle()])
})

function vrBundle(overrides: Partial<CollectionSummary> = {}): CollectionSummary {
  return {
    id: 'vr-starter',
    displayName: 'VR Starter Pack',
    category: 'vr',
    description: 'The three things you need before first playthrough.',
    targetGame: null,
    capabilityCount: 2,
    requiredCount: 1,
    preset: {
      installDir: 'C:\\games\\elden-ring\\Mods',
      executableDir: 'C:\\games\\elden-ring',
      capabilities: [
        { id: 'uevr', displayName: 'UEVR', rationale: 'Universal VR injector.' },
        { id: 'vd', displayName: 'VirtualDesktop', rationale: 'Runtime and profiles.' },
      ],
    },
    ...overrides,
  }
}

async function openPanel() {
  const wrapper = mount(CollectionsPanel, {
    attachTo: document.body,
    global: { plugins: [i18n] },
  })
  await wrapper.find('button.nav-item').trigger('click')
  await flushPromises()
  return wrapper
}

function dialogs() {
  return Array.from(document.body.querySelectorAll('[role="dialog"]'))
}

function rowInstallButton() {
  return document.body.querySelector('.collections-entry-actions button') as HTMLButtonElement
}

/** The plan view's own "Install collection" button, in the install dialog. */
function planInstallButton() {
  return Array.from(document.body.querySelectorAll('.dialog-footer .btn-primary')).find(
    (button) => button.textContent?.includes('Install collection'),
  ) as HTMLButtonElement | undefined
}

function confirmButton() {
  return document.body.querySelector('.dialog-footer .btn-danger-solid') as HTMLButtonElement | null
}

/** Row → install dialog. Stops there; nothing has been confirmed yet. */
async function askInstall() {
  rowInstallButton().click()
  await new Promise((resolve) => setTimeout(resolve, 0))
}

/** Row → install dialog → the "this writes to your game folder" gate. */
async function requestInstall() {
  await askInstall()
  planInstallButton()?.click()
  await new Promise((resolve) => setTimeout(resolve, 0))
}

describe('CollectionsPanel — the install path', () => {
  it('installs a collection through the ordinary capability install command', async () => {
    await openPanel()
    await requestInstall()
    confirmButton()?.click()
    await flushPromises()

    // Two capabilities, two calls, in the order the preset lists them.
    expect(mockedInstall).toHaveBeenCalledTimes(2)
    expect(mockedInstall).toHaveBeenNthCalledWith(1, {
      capabilityId: 'uevr',
      gameId: 'elden-ring',
      gameName: 'Elden Ring',
      installDir: 'C:\\games\\elden-ring\\Mods',
      executableDir: 'C:\\games\\elden-ring',
      config: {},
      // Fail closed: a collection may not wave through an unsigned recipe.
      acceptUnsigned: false,
    })
    expect(mockedInstall).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({ capabilityId: 'vd' }),
    )

    // The branch this came from had its own collection install command
    // behind a Rust session runner. That module is not in this build, and
    // reviving it would be the second install path the architecture
    // forbids. A second path would have to reach the Tauri bridge, and
    // jsdom has no bridge to reach: the run would stall and never reach
    // the completed state asserted here. Both capabilities reporting
    // "installed" is therefore the proof that one command did the work.
    const steps = Array.from(dialogs()[0]?.querySelectorAll('.collection-step-list li') ?? []).map(
      (step) => step.textContent,
    )
    expect(steps).toHaveLength(2)
    expect(steps[0]).toContain('UEVR')
    expect(steps[0]).toContain('installed')
    expect(steps[1]).toContain('VirtualDesktop')
    expect(steps[1]).toContain('installed')
    expect(dialogs()[0]?.textContent).toContain('Collection installed')
  })

  it('confirms before writing, names the cost, and does not autofocus the confirm button', async () => {
    await openPanel()
    await requestInstall()

    // The gate is up and the command has not fired.
    expect(mockedInstall).not.toHaveBeenCalled()
    expect(confirmButton()).not.toBeNull()
    const details = Array.from(document.body.querySelectorAll('.confirm-details li')).map(
      (item) => item.textContent,
    )
    expect(details[0]).toContain('2 capabilities')
    expect(details[0]).toContain('Elden Ring')
    expect(details.join(' ')).toContain('can be replaced')
    expect(details.join(' ')).toContain('History')

    // ConfirmDialog never autofocuses: a dialog opening over a game
    // folder must not remove a mod because the user pressed Enter.
    expect(document.activeElement).not.toBe(confirmButton())
    expect((confirmButton() as HTMLButtonElement).tagName).toBe('BUTTON')
  })

  it('dismisses the confirmation on Escape without issuing the command', async () => {
    await openPanel()
    await requestInstall()
    expect(confirmButton()).not.toBeNull()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    expect(confirmButton()).toBeNull()
    expect(mockedInstall).not.toHaveBeenCalled()
    // Control: the wizard behind the confirmation survives, so Escape
    // dismissed the top dialog and not the whole install.
    expect(dialogs()).toHaveLength(1)
    expect(dialogs()[0]?.textContent).toContain('UEVR')
  })

  it('reverts what the run installed, newest first, through the capability uninstall', async () => {
    // The first capability lands; the second one cannot.
    mockedInstall
      .mockResolvedValueOnce({
        capabilityId: 'uevr',
        transaction: { id: 'tx-1' },
        steps: [],
        affectedPaths: [],
      } as CommunityInstallResult)
      .mockRejectedValueOnce(new Error('VirtualDesktop needs a writable folder'))
    await openPanel()
    await requestInstall()
    confirmButton()?.click()
    await flushPromises()

    // Stopped on the second step with the first one already on disk.
    expect(mockedUninstall).not.toHaveBeenCalled()
    const revert = Array.from(document.body.querySelectorAll('.dialog-footer button')).find(
      (button) => button.textContent?.includes('Revert'),
    ) as HTMLButtonElement
    revert.click()
    await flushPromises()

    expect(mockedUninstall).toHaveBeenCalledTimes(1)
    expect(mockedUninstall).toHaveBeenCalledWith({
      capabilityId: 'uevr',
      gameId: 'elden-ring',
      installDir: 'C:\\games\\elden-ring\\Mods',
    })
  })
})

describe('CollectionsPanel — states', () => {
  it('renders an empty catalog through EmptyState', async () => {
    mockedList.mockResolvedValue([])
    await openPanel()

    const empty = document.body.querySelector('.empty-state')
    expect(empty).not.toBeNull()
    expect(empty?.getAttribute('role')).toBe('status')
    expect(empty?.textContent).toContain('No collections available yet')
    expect(document.body.querySelector('.collections-list')).toBeNull()
  })

  it('shows a busy state instead of an empty list while the catalog loads', async () => {
    let release = () => {}
    mockedList.mockReturnValue(new Promise((resolve) => { release = () => resolve([vrBundle()]) }))
    const wrapper = mount(CollectionsPanel, { attachTo: document.body, global: { plugins: [i18n] } })
    await wrapper.find('button.nav-item').trigger('click')
    await flushPromises()

    expect(document.body.querySelector('.empty-state')?.getAttribute('aria-busy')).toBe('true')

    release()
    await flushPromises()
    expect(document.body.querySelector('.collections-entry')).not.toBeNull()
  })

  it('surfaces a failed load through ErrorCallout, not the raw message', async () => {
    mockedList.mockRejectedValue(new Error('The catalog index is corrupt at byte 4096'))
    await openPanel()

    const callout = document.body.querySelector('.callout-danger')
    expect(callout).not.toBeNull()
    expect(callout?.getAttribute('role')).toBe('alert')
    // Titled and explained, with the untranslated string one disclosure
    // away rather than shown as the explanation.
    expect(callout?.querySelector('strong')?.textContent).toBe('Collections could not be loaded')
    expect(callout?.querySelector('p')?.textContent).toBe(
      'Moddin could not read the collection catalog. Nothing was changed.',
    )
    expect(callout?.querySelector('p')?.textContent).not.toContain('corrupt at byte 4096')
    expect(callout?.querySelector('details.callout-raw code')?.textContent).toContain(
      'corrupt at byte 4096',
    )
  })

  it('surfaces a failed install through ErrorCallout, not the raw message', async () => {
    mockedInstall.mockRejectedValue(new Error('0x80070005: access denied to C:\\games\\elden-ring'))
    await openPanel()
    await requestInstall()
    confirmButton()?.click()
    await flushPromises()

    const callout = document.body.querySelector('.callout-danger')
    expect(callout).not.toBeNull()
    expect(callout?.querySelector('p')?.textContent).toBe(
      'Some capabilities may already be installed. Check History before trying again.',
    )
    expect(callout?.querySelector('p')?.textContent).not.toContain('0x80070005')
    // The run stopped and offers the two ways out, rather than carrying on.
    expect(dialogs()[0]?.textContent).toContain('Revert everything that was installed')
  })

  it('refuses to install a collection that has nothing attached to it', async () => {
    mockedList.mockResolvedValue([vrBundle({ preset: undefined })])
    await openPanel()

    expect(rowInstallButton().disabled).toBe(true)
    await askInstall()
    expect(dialogs()).toHaveLength(1)
    expect(mockedInstall).not.toHaveBeenCalled()  })
})
