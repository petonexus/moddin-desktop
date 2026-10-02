import { shallowRef } from 'vue'
import { selectedGameKey, type SelectedGameContext } from '../../../composables/useSelectedGame'
const selection = shallowRef<SelectedGameContext | null>(null)
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import CommunityPanel from '../CommunityPanel.vue'
import { i18n } from '../../../i18n'
import type { CommunityCatalogEntry, CommunityFetchResult } from '../../../types/community'
import type { CommunityInstallResult } from '../types'

/**
 * The Community panel's install path, which is the roadmap item this
 * change exists for: a community recipe that declares required config
 * could not be installed from here, because the panel sent
 * `config: {}` — which `ResolvedConfig` rejects before the run starts.
 *
 * Two things are pinned. A field nobody can answer is *asked for* rather
 * than sent blank, and a field the catalogue does answer is filled in and
 * sent. The pre-fill is the same resolver the game page's cards use, so it
 * reads the per-game `config:` block rather than a second table.
 */
vi.mock('../service', () => ({
  fetchCommunityCatalog: vi.fn(),
  installCommunityCapability: vi.fn(),
  listCapabilities: vi.fn(),
  reloadCapabilities: vi.fn(),
  setCommunityCatalogTtl: vi.fn(),
}))
vi.mock('../../capability-modules/service', () => ({ resolveInstallTarget: vi.fn() }))
vi.mock('../../../composables/useAiAssistant', () => ({ recordUnsignedConsent: vi.fn() }))

import {
  fetchCommunityCatalog,
  installCommunityCapability,
  listCapabilities,
  reloadCapabilities,
  setCommunityCatalogTtl,
} from '../service'
import { resolveInstallTarget } from '../../capability-modules/service'

const mockedFetch = vi.mocked(fetchCommunityCatalog)
const mockedInstall = vi.mocked(installCommunityCapability)
const mockedList = vi.mocked(listCapabilities)
const mockedReload = vi.mocked(reloadCapabilities)
const mockedTtl = vi.mocked(setCommunityCatalogTtl)
const mockedResolve = vi.mocked(resolveInstallTarget)

enableAutoUnmount(afterEach)

afterEach(() => {
  document.body.innerHTML = ''
})

/** The shipped community entry's schema, field for field. */
const FPS_UNLOCKER_SCHEMA = [
  { name: 'downloadUrl', type: 'url', required: true, description: 'HTTPS URL of the release archive (zip).' },
  { name: 'sha256', type: 'sha256', required: true },
  { name: 'processName', type: 'string', required: true, description: 'Asked for, not guessed.' },
]

function entry(overrides: Partial<CommunityCatalogEntry> = {}): CommunityCatalogEntry {
  return {
    id: 'community-fps-unlocker',
    version: '1.0.0',
    displayName: 'FPS Unlocker',
    category: 'graphics',
    status: 'planned',
    homepage: null,
    downloadUrl: 'https://example.invalid/capability.yaml',
    configSchema: FPS_UNLOCKER_SCHEMA,
    safetyNotes: [],
    signed: true,
    ...overrides,
  }
}

function fetchResult(entries: CommunityCatalogEntry[]): CommunityFetchResult {
  return {
    catalog: { version: 1, generatedAt: '2026-01-01', generator: 'test', capabilities: entries, revoked: [] },
    cached: true,
    cachedAt: 1_700_000_000_000,
    ttlSeconds: 86_400,
    signatureVerified: true,
    bootstrapPublicKeyFingerprint: 'ABCDEF0123456789',
    lastError: null,
  }
}

async function openPanel(entries: CommunityCatalogEntry[], gameId = 'doom-2016') {
  selection.value = { appId: '1245620', gameId, gameName: 'Elden Ring', engine: null }
  mockedResolve.mockResolvedValue({
    gameId,
    gameName: 'Cyberpunk 2077',
    installDir: 'C:\\games\\cyberpunk\\Mods',
    executableDir: 'C:\\games\\cyberpunk',
  })
  mockedFetch.mockResolvedValue(fetchResult(entries))
  const wrapper = mount(CommunityPanel, { attachTo: document.body, global: { plugins: [i18n], provide: { [selectedGameKey as symbol]: selection } } })
  await wrapper.find('button.nav-item').trigger('click')
  await flushPromises()
  return wrapper
}

function installButton() {
  return document.body.querySelector('.community-entry-actions button') as HTMLButtonElement
}

function field(capabilityId: string, name: string): HTMLInputElement {
  return document.body.querySelector(`#community-config-${capabilityId}-${name}`) as HTMLInputElement
}

/** Type into a rendered input the way a person does. */
async function type(capabilityId: string, name: string, value: string) {
  const input = field(capabilityId, name)
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
  await flushPromises()
}

/** Tick a rendered checkbox the way a person does. */
async function tick(selector: string) {
  const box = document.body.querySelector(selector) as HTMLInputElement
  box.checked = !box.checked
  box.dispatchEvent(new Event('change', { bubbles: true }))
  await flushPromises()
}

beforeEach(() => {
  i18n.global.locale.value = 'en'
  mockedFetch.mockReset()
  mockedInstall.mockReset()
  mockedList.mockReset()
  mockedReload.mockReset()
  mockedTtl.mockReset()
  mockedResolve.mockReset()

  mockedTtl.mockResolvedValue(0)
  mockedReload.mockResolvedValue([])
  mockedList.mockResolvedValue([])
  mockedInstall.mockResolvedValue({
    capabilityId: 'community-fps-unlocker',
    transaction: { id: 'tx-1' },
    steps: [],
    affectedPaths: [],
  } as CommunityInstallResult)
})

describe('a community recipe with required config', () => {
  it('installs for the live library selection even if legacy storage names another game', async () => {
    window.localStorage.setItem('moddin-selected-appId', 'wrong-game')
    await openPanel([entry({ status: 'available', configSchema: [] })], 'elden-ring')
    selection.value = { appId: '1091500', gameId: 'cyberpunk-2077', gameName: 'Cyberpunk 2077', engine: null }
    mockedResolve.mockResolvedValue({ gameId: 'cyberpunk-2077', gameName: 'Cyberpunk 2077',
      installDir: 'C:\\games\\cyberpunk', executableDir: 'C:\\games\\cyberpunk' })
    await flushPromises()
    installButton().click()
    await flushPromises()
    expect(mockedResolve).toHaveBeenCalledWith('cyberpunk-2077')
    expect(mockedInstall).toHaveBeenCalledWith(expect.objectContaining({ gameId: 'cyberpunk-2077' }))
    window.localStorage.removeItem('moddin-selected-appId')
  })

  it('keeps the dialog open while the install is running', async () => {
    let complete!: (value: CommunityInstallResult) => void
    mockedInstall.mockImplementation(() => new Promise((resolve) => { complete = resolve }))
    await openPanel([entry({ status: 'available', configSchema: [] })])
    installButton().click()
    await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(document.body.querySelector('[role="dialog"]')).not.toBeNull()
    complete({ capabilityId: 'community-fps-unlocker', transaction: null, steps: [], affectedPaths: [] })
    await flushPromises()
  })

  it('renders the recipe\'s own fields rather than a button that cannot work', async () => {
    await openPanel([entry()])

    expect(field('community-fps-unlocker', 'downloadUrl')).not.toBeNull()
    expect(document.body.textContent).toContain('processName')
  })

  it('refuses the install and names the empty fields rather than sending a blank', async () => {
    await openPanel([entry()])
    installButton().click()
    await flushPromises()

    // No install call. The failure the user would otherwise have seen is
    // a `download-file` step complaining about a field it was never shown.
    expect(mockedInstall).not.toHaveBeenCalled()
    const callout = document.body.querySelector('.callout-danger')
    expect(callout?.textContent).toContain('FPS Unlocker')
    expect(callout?.textContent).toContain('sha256')
    expect(callout?.textContent).toContain('processName')
    // And the fields say so, for a user who looks at the form.
    expect(field('community-fps-unlocker', 'sha256').getAttribute('aria-invalid')).toBe('true')
    expect(field('community-fps-unlocker', 'downloadUrl').getAttribute('aria-invalid')).toBe('true')
  })

  it('does not flag a field the user has not tried to install yet', async () => {
    // UX-28, the same rule the game page's cards follow: a form full of
    // `aria-invalid` on first paint describes mistakes nobody has made.
    await openPanel([entry()])

    expect(field('community-fps-unlocker', 'sha256').getAttribute('aria-invalid')).toBeNull()
  })

  it('installs with the values the user completed, wrapped as the backend expects', async () => {
    await openPanel([entry()])
    await type('community-fps-unlocker', 'downloadUrl', 'https://example.invalid/fpsunlocker.zip')
    await type('community-fps-unlocker', 'sha256', 'abc123')
    await type('community-fps-unlocker', 'processName', 'DOOMx64')

    installButton().click()
    await flushPromises()

    expect(mockedInstall).toHaveBeenCalledWith({
      capabilityId: 'community-fps-unlocker',
      gameId: 'doom-2016',
      gameName: 'Cyberpunk 2077',
      installDir: 'C:\\games\\cyberpunk\\Mods',
      executableDir: 'C:\\games\\cyberpunk',
      // `{ values: … }`, not `{}`: `ResolvedConfig` is a wrapper, and the
      // empty object the panel used to send failed to deserialise.
      config: {
        values: {
          downloadUrl: 'https://example.invalid/fpsunlocker.zip',
          sha256: 'abc123',
          processName: 'DOOMx64',
        },
      },
      acceptUnsigned: false,
    })
  })

  it('pre-fills what the catalogue already knows for the selected game', async () => {
    // A community entry that carries the same capability id as a module the
    // catalogue already describes — a community build of a recipe Moddin
    // ships. The pre-fill comes from the same per-game `config:` block the
    // game page's own card uses, read out of the real shipped catalogue.
    await openPanel(
      [
        entry({
          id: 'optiscaler',
          displayName: 'OptiScaler (community build)',
          configSchema: [
            { name: 'downloadUrl', type: 'url', required: true },
            { name: 'version', type: 'string', required: true },
          ],
        }),
      ],
      'cyberpunk-2077',
    )

    expect(field('optiscaler', 'downloadUrl').value).toBe(
      'https://github.com/optiscaler/OptiScaler/releases/download/v0.9.4/Optiscaler_0.9.4-final.20260718._MM.7z',
    )

    // Nothing left to ask for, so the install goes straight through.
    installButton().click()
    await flushPromises()

    expect(mockedInstall).toHaveBeenCalledWith(
      expect.objectContaining({
        config: {
          values: {
            downloadUrl: 'https://github.com/optiscaler/OptiScaler/releases/download/v0.9.4/Optiscaler_0.9.4-final.20260718._MM.7z',
            version: '0.9.4',
          },
        },
      }),
    )
  })

  it('installs a recipe with no required fields without a detour', async () => {
    await openPanel([entry({ configSchema: [{ name: 'tweak', type: 'boolean', required: false }] })])
    installButton().click()
    await flushPromises()

    expect(mockedInstall).toHaveBeenCalledTimes(1)
  })
})

describe('a recipe with an unverified signature', () => {
  it('still asks for consent before anything is installed', async () => {
    // A recipe with nothing required, so the only gate left is consent.
    await openPanel([entry({ signed: false, configSchema: [{ name: 'tweak', type: 'boolean', required: false }] })])
    expect(installButton().disabled).toBe(true)

    await tick('.community-consent input')
    installButton().click()
    await flushPromises()

    expect(mockedInstall).toHaveBeenCalledWith(
      expect.objectContaining({ acceptUnsigned: true, config: { values: { tweak: false } } }),
    )
  })
})
